use std::{
    fmt::Debug,
    net::{
        Ipv4Addr,
        Ipv6Addr,
    },
    os::fd::{
        AsRawFd,
        OwnedFd,
    },
    sync::Arc,
};

pub use nix::net::if_::InterfaceFlags as Flags;
use nix::sys::socket::{
    AddressFamily,
    LinkAddr,
    MsgFlags,
    SockFlag,
    SockProtocol,
    SockType,
};
use smallvec::SmallVec;
use tokio::io::{
    unix::AsyncFd,
    Interest,
};

use super::{
    packet::{
        PacketListener,
        PacketSender,
    },
    MacAddress,
};

/// A network interface.
#[derive(Clone)]
pub struct Interface {
    first: Arc<nix::ifaddrs::InterfaceAddress>,
    link: Link,
    ipv4: SmallVec<[Ipv4; 1]>,
    ipv6: SmallVec<[Ipv6; 1]>,
}

impl Interface {
    fn new(addresses: &[Arc<nix::ifaddrs::InterfaceAddress>]) -> Self {
        let first = addresses
            .first()
            .expect("trying to create an interface with no addresses")
            .clone();
        for addr in &addresses[1..] {
            assert_eq!(addr.interface_name, first.interface_name);
            assert_eq!(addr.flags, first.flags);
        }

        let mut link = None;
        let mut ipv4 = SmallVec::new();
        let mut ipv6 = SmallVec::new();

        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Kind {
            Link,
            Ipv4,
            Ipv6,
        }

        fn kind(addr: &nix::sys::socket::SockaddrStorage) -> Option<Kind> {
            if addr.as_link_addr().is_some() {
                Some(Kind::Link)
            }
            else if addr.as_sockaddr_in().is_some() {
                Some(Kind::Ipv4)
            }
            else if addr.as_sockaddr_in6().is_some() {
                Some(Kind::Ipv6)
            }
            else {
                None
            }
        }

        for addr in addresses {
            let addrs = [
                &addr.address,
                &addr.netmask,
                &addr.broadcast,
                &addr.destination,
            ];
            let mut k1 = None;
            for a in addrs {
                if let Some(a) = a {
                    if let Some(k2) = kind(a) {
                        if let Some(k1) = k1 {
                            assert_eq!(k1, k2, "mixed addresses");
                        }
                        else {
                            k1 = Some(k2);
                        }
                    }
                }
            }

            match k1 {
                Some(Kind::Link) => {
                    // we only want one link layer config, and address must be set.
                    if let Some(address) = &addr.address {
                        let link_addr = *address.as_link_addr().unwrap();
                        if link_addr.addr().is_some() {
                            assert!(
                                link.is_none(),
                                "interface with multiple link layer addresses"
                            );
                            link = Some(Link {
                                interface: addr.clone(),
                                link_addr,
                                net_mask: addr
                                    .netmask
                                    .and_then(|a| a.as_link_addr().unwrap().addr())
                                    .map(Into::into),
                                broadcast: addr
                                    .broadcast
                                    .and_then(|a| a.as_link_addr().unwrap().addr())
                                    .map(Into::into),
                                destination: addr
                                    .destination
                                    .and_then(|a| a.as_link_addr().unwrap().addr())
                                    .map(Into::into),
                            });
                        }
                    }
                }
                Some(Kind::Ipv4) => {
                    ipv4.push(Ipv4 {
                        interface: addr.clone(),
                        address: addr.address.map(|a| *a.as_sockaddr_in().unwrap()),
                        net_mask: addr.netmask.map(|a| *a.as_sockaddr_in().unwrap()),
                        broadcast: addr.broadcast.map(|a| *a.as_sockaddr_in().unwrap()),
                        destination: addr.destination.map(|a| *a.as_sockaddr_in().unwrap()),
                    });
                }
                Some(Kind::Ipv6) => {
                    ipv6.push(Ipv6 {
                        interface: addr.clone(),
                        address: addr.address.map(|a| *a.as_sockaddr_in6().unwrap()),
                        net_mask: addr.netmask.map(|a| *a.as_sockaddr_in6().unwrap()),
                        broadcast: addr.broadcast.map(|a| *a.as_sockaddr_in6().unwrap()),
                        destination: addr.destination.map(|a| *a.as_sockaddr_in6().unwrap()),
                    });
                }
                None => todo!(),
            }
        }

        Self {
            first,
            link: link.expect("interface with no link layer address"),
            ipv4,
            ipv6,
        }
    }

    pub fn enumerate() -> Result<Vec<Interface>, std::io::Error> {
        let mut addresses = nix::ifaddrs::getifaddrs()?
            .map(Arc::new)
            .collect::<Vec<_>>();
        addresses.sort_by(|a, b| a.interface_name.cmp(&b.interface_name));
        let n = addresses.len();
        let mut it = addresses.into_iter();

        let Some(mut prev) = it.next()
        else {
            return Ok(vec![]);
        };
        let mut buf = Vec::with_capacity(n);
        buf.push(prev.clone());

        let mut interfaces = Vec::with_capacity(n);

        while let Some(next) = it.next() {
            if next.interface_name != prev.interface_name {
                interfaces.push(Self::new(&buf));
                buf.clear();
                prev = next.clone();
            }
            buf.push(next);
        }

        if !buf.is_empty() {
            // buf is empty if there was only 1 interface to begin with.
            interfaces.push(Self::new(&buf));
        }

        interfaces.shrink_to_fit();
        Ok(interfaces)
    }

    pub fn from_name(name: &str) -> Option<Interface> {
        let mut buf = vec![];
        for address in nix::ifaddrs::getifaddrs().ok()? {
            if address.interface_name == name {
                buf.push(Arc::new(address))
            }
        }
        if buf.is_empty() {
            None
        }
        else {
            Some(Self::new(&buf))
        }
    }

    pub fn link(&self) -> &Link {
        &self.link
    }

    pub fn ipv4(&self) -> ConfigIter<'_, Ipv4> {
        ConfigIter {
            inner: self.ipv4.iter(),
        }
    }

    pub fn ipv6(&self) -> ConfigIter<'_, Ipv6> {
        ConfigIter {
            inner: self.ipv6.iter(),
        }
    }

    #[inline]
    pub fn name(&self) -> &str {
        &self.first.interface_name
    }

    #[inline]
    pub fn flags(&self) -> Flags {
        self.first.flags
    }

    #[inline]
    pub fn hardware_address(&self) -> MacAddress {
        self.link.address()
    }

    pub fn if_index(&self) -> usize {
        self.link.link_addr.ifindex()
    }

    /// A LinkAddr we can use to bind a raw socket with
    #[inline]
    fn raw_bind_address(&self) -> Option<nix::sys::socket::LinkAddr> {
        self.link.interface.address?.as_link_addr().copied()
    }

    pub fn socket(&self) -> Result<Socket, std::io::Error> {
        Socket::open(self.clone())
    }
}

impl Debug for Interface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Interface")
            .field("link", &self.link)
            .field("ipv4", &self.ipv4)
            .field("ipv6", &self.ipv6)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IpVersion {
    V4,
    V6,
}

#[derive(Clone)]
pub struct Link {
    interface: Arc<nix::ifaddrs::InterfaceAddress>,
    link_addr: LinkAddr,
    net_mask: Option<MacAddress>,
    broadcast: Option<MacAddress>,
    destination: Option<MacAddress>,
}

impl Link {
    #[inline]
    pub fn protocol(&self) -> u16 {
        self.link_addr.protocol()
    }

    #[inline]
    pub fn hardware_type(&self) -> u16 {
        self.link_addr.hatype()
    }

    #[inline]
    pub fn packet_type(&self) -> u8 {
        self.link_addr.pkttype()
    }

    #[inline]
    pub fn address(&self) -> MacAddress {
        self.link_addr.addr().unwrap().into()
    }

    #[inline]
    pub fn net_mask(&self) -> Option<MacAddress> {
        self.net_mask
    }

    #[inline]
    pub fn broadcast(&self) -> Option<MacAddress> {
        self.broadcast
    }

    #[inline]
    pub fn destination(&self) -> Option<MacAddress> {
        self.destination
    }
}

impl Debug for Link {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("protocol", &self.protocol())
            .field("hardware_type", &self.hardware_type())
            .field("packet_type", &self.packet_type())
            .field("address", &self.address())
            .field("net_mask", &self.net_mask)
            .field("broadcast", &self.broadcast)
            .field("destination", &self.destination)
            .finish()
    }
}

#[derive(Clone)]
pub struct Ipv4 {
    interface: Arc<nix::ifaddrs::InterfaceAddress>,
    address: Option<nix::sys::socket::SockaddrIn>,
    net_mask: Option<nix::sys::socket::SockaddrIn>,
    broadcast: Option<nix::sys::socket::SockaddrIn>,
    destination: Option<nix::sys::socket::SockaddrIn>,
}

impl Ipv4 {
    #[inline]
    pub fn address(&self) -> Option<Ipv4Addr> {
        self.address.map(|a| a.ip())
    }

    #[inline]
    pub fn net_mask(&self) -> Option<Ipv4Addr> {
        self.net_mask.map(|a| a.ip())
    }

    #[inline]
    pub fn broadcast(&self) -> Option<Ipv4Addr> {
        self.broadcast.map(|a| a.ip())
    }

    #[inline]
    pub fn destination(&self) -> Option<Ipv4Addr> {
        self.destination.map(|a| a.ip())
    }

    #[inline]
    pub(crate) fn interface(&self) -> &nix::ifaddrs::InterfaceAddress {
        &self.interface
    }
}

impl Debug for Ipv4 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ip")
            .field("address", &self.address())
            .field("net_mask", &self.net_mask())
            .field("broadcast", &self.broadcast())
            .field("destination", &self.destination())
            .finish()
    }
}

#[derive(Clone)]
pub struct Ipv6 {
    interface: Arc<nix::ifaddrs::InterfaceAddress>,
    address: Option<nix::sys::socket::SockaddrIn6>,
    net_mask: Option<nix::sys::socket::SockaddrIn6>,
    broadcast: Option<nix::sys::socket::SockaddrIn6>,
    destination: Option<nix::sys::socket::SockaddrIn6>,
}

impl Ipv6 {
    pub fn address(&self) -> Option<Ipv6Addr> {
        self.address.map(|a| a.ip())
    }

    pub fn net_mask(&self) -> Option<Ipv6Addr> {
        self.net_mask.map(|a| a.ip())
    }

    pub fn broadcast(&self) -> Option<Ipv6Addr> {
        self.broadcast.map(|a| a.ip())
    }

    pub fn destination(&self) -> Option<Ipv6Addr> {
        self.destination.map(|a| a.ip())
    }

    pub(crate) fn interface(&self) -> &nix::ifaddrs::InterfaceAddress {
        &self.interface
    }
}

impl Debug for Ipv6 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ip")
            .field("address", &self.address())
            .field("net_mask", &self.net_mask())
            .field("broadcast", &self.broadcast())
            .field("destination", &self.destination())
            .finish()
    }
}

pub struct ConfigIter<'a, T> {
    inner: std::slice::Iter<'a, T>,
}

impl<'a, T> Iterator for ConfigIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<'a, T> DoubleEndedIterator for ConfigIter<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back()
    }
}

impl<'a, T> ExactSizeIterator for ConfigIter<'a, T> {}

/// A "raw" socket. This can be used to send and receive ethernet frames.[1]
///
/// This can be created with [`Interface::socket`]
///
/// [1]: https://www.man7.org/linux/man-pages/man7/packet.7.html
#[derive(Debug)]
pub struct Socket {
    socket: AsyncFd<OwnedFd>,
    interface: Interface,
}

impl Socket {
    fn open(interface: Interface) -> Result<Self, std::io::Error> {
        // note: the returned OwnedFd will close the socket on drop.
        let socket = nix::sys::socket::socket(
            AddressFamily::Packet,
            SockType::Raw,
            SockFlag::SOCK_NONBLOCK,
            SockProtocol::EthAll,
        )?;

        let bind_address = interface
            .raw_bind_address()
            .expect("interface has no address we can bind to");
        // note: we might need to set the protocol field in `bind_addr`, but this is
        // currently not possible. see https://github.com/nix-rust/nix/issues/2059
        nix::sys::socket::bind(socket.as_raw_fd(), &bind_address)?;

        let socket = AsyncFd::with_interest(socket, Interest::READABLE | Interest::WRITABLE)?;

        Ok(Self { socket, interface })
    }

    /// Receives a packet from the interface.
    ///
    /// The real packet size is returned, even if the buffer wasn't large
    /// enough.
    pub async fn receive(&self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        self.socket
            .async_io(Interest::READABLE, |socket| {
                // MsgFlags::MSG_TRUNC tells the kernel to return the real packet length, even
                // if the buffer is not large enough.
                Ok(nix::sys::socket::recv(
                    socket.as_raw_fd(),
                    buf,
                    MsgFlags::MSG_TRUNC,
                )?)
            })
            .await
    }

    pub async fn send(&self, buf: &[u8]) -> Result<(), std::io::Error> {
        // can we send one packet with multiple `send` calls? then we can send `impl
        // Buf`.
        let bytes_sent = self
            .socket
            .async_io(Interest::WRITABLE, |socket| {
                Ok(nix::sys::socket::send(
                    socket.as_raw_fd(),
                    buf,
                    MsgFlags::empty(),
                )?)
            })
            .await?;
        if bytes_sent < buf.len() {
            tracing::warn!(buf_len = buf.len(), bytes_sent, "sent truncated packet");
        }
        Ok(())
    }

    pub fn interface(&self) -> &Interface {
        &self.interface
    }

    // todo: remove this
    pub fn into_pair(self) -> (PacketListener, PacketSender) {
        let this = Arc::new(self);
        (PacketListener::new(this.clone()), PacketSender::new(this))
    }
}
