use std::{
    fmt::Debug,
    net::{
        Ipv4Addr,
        Ipv6Addr,
    },
    sync::Arc,
};

use ip_network::{
    Ipv4Network,
    Ipv6Network,
};
pub use nix::net::if_::InterfaceFlags as Flags;
use nix::{
    ifaddrs::InterfaceAddress,
    sys::socket::LinkAddr,
};
use smallvec::SmallVec;

use super::socket::{
    Mode,
    Receiver,
    Sender,
    Socket,
};
use crate::protocol::inet::MacAddress;

struct Inner {
    first: nix::ifaddrs::InterfaceAddress,
    link: Link,
    ipv4: SmallVec<[Ipv4; 1]>,
    ipv6: SmallVec<[Ipv6; 1]>,
}

/// A network interface.
#[derive(Clone)]
pub struct Interface {
    inner: Arc<Inner>,
}

impl Interface {
    fn new(addresses: Vec<nix::ifaddrs::InterfaceAddress>) -> Self {
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

                            let net_mask = addr
                                .netmask
                                .and_then(|a| a.as_link_addr().unwrap().addr())
                                .map(Into::into);

                            let broadcast = addr
                                .broadcast
                                .and_then(|a| a.as_link_addr().unwrap().addr())
                                .map(Into::into);

                            let destination = addr
                                .destination
                                .and_then(|a| a.as_link_addr().unwrap().addr())
                                .map(Into::into);

                            link = Some(Link {
                                interface: addr,
                                link_addr,
                                net_mask,
                                broadcast,
                                destination,
                            });
                        }
                    }
                }
                Some(Kind::Ipv4) => {
                    let address = addr.address.map(|a| a.as_sockaddr_in().unwrap().ip());
                    let net_mask = addr
                        .netmask
                        .map(|a| a.as_sockaddr_in().unwrap().ip().into());
                    let broadcast = addr.broadcast.map(|a| a.as_sockaddr_in().unwrap().ip());
                    let destination = addr.destination.map(|a| a.as_sockaddr_in().unwrap().ip());
                    ipv4.push(Ipv4 {
                        interface: addr,
                        address,
                        net_mask,
                        broadcast,
                        destination,
                    });
                }
                Some(Kind::Ipv6) => {
                    let address = addr.address.map(|a| a.as_sockaddr_in6().unwrap().ip());
                    let net_mask = addr
                        .netmask
                        .map(|a| a.as_sockaddr_in6().unwrap().ip().into());
                    let broadcast = addr.broadcast.map(|a| a.as_sockaddr_in6().unwrap().ip());
                    let destination = addr.destination.map(|a| a.as_sockaddr_in6().unwrap().ip());
                    ipv6.push(Ipv6 {
                        interface: addr,
                        address,
                        net_mask,
                        broadcast,
                        destination,
                    });
                }
                None => todo!(),
            }
        }

        Self {
            inner: Arc::new(Inner {
                first,
                link: link.expect("interface with no link layer address"),
                ipv4,
                ipv6,
            }),
        }
    }

    pub fn list() -> Result<Vec<Interface>, std::io::Error> {
        let mut addresses = nix::ifaddrs::getifaddrs()?.collect::<Vec<_>>();
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
                interfaces.push(Self::new(std::mem::take(&mut buf)));
                prev = next.clone();
            }
            buf.push(next);
        }

        if !buf.is_empty() {
            // buf is empty if there was only 1 interface to begin with.
            interfaces.push(Self::new(buf));
        }

        interfaces.shrink_to_fit();
        Ok(interfaces)
    }

    pub fn from_name(name: &str) -> Option<Interface> {
        let mut buf = vec![];
        for address in nix::ifaddrs::getifaddrs().ok()? {
            if address.interface_name == name {
                buf.push(address)
            }
        }
        if buf.is_empty() {
            None
        }
        else {
            Some(Self::new(buf))
        }
    }

    pub fn link(&self) -> &Link {
        &self.inner.link
    }

    pub fn ipv4(&self) -> ConfigIter<'_, Ipv4> {
        ConfigIter {
            inner: self.inner.ipv4.iter(),
        }
    }

    pub fn ipv6(&self) -> ConfigIter<'_, Ipv6> {
        ConfigIter {
            inner: self.inner.ipv6.iter(),
        }
    }

    #[inline]
    pub fn name(&self) -> &str {
        &self.inner.first.interface_name
    }

    #[inline]
    pub fn flags(&self) -> Flags {
        self.inner.first.flags
    }

    #[inline]
    pub fn hardware_address(&self) -> MacAddress {
        self.inner.link.address()
    }

    pub fn if_index(&self) -> usize {
        self.inner.link.link_addr.ifindex()
    }

    /// A LinkAddr we can use to bind a raw socket with
    #[inline]
    pub(super) fn raw_bind_address(&self) -> Option<nix::sys::socket::LinkAddr> {
        self.inner.link.interface.address?.as_link_addr().copied()
    }

    #[inline]
    pub fn socket(&self, mode: Mode) -> Result<Socket, std::io::Error> {
        Socket::open(self.clone(), mode)
    }

    #[inline]
    pub fn channel(&self, mode: Mode) -> Result<(Sender, Receiver), std::io::Error> {
        Ok(self.socket(mode)?.into_channel())
    }
}

impl Debug for Interface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("Interface");
        s.field("name", &self.name())
            .field("link", &self.inner.link);
        if !self.inner.ipv4.is_empty() {
            s.field("ipv4", &self.inner.ipv4);
        }
        if !self.inner.ipv6.is_empty() {
            s.field("ipv6", &self.inner.ipv6);
        }
        s.finish()
    }
}

#[derive(Clone)]
pub struct Link {
    interface: InterfaceAddress,
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
        let mut s = f.debug_struct("Link");
        s.field("protocol", &self.protocol())
            .field("hardware_type", &self.hardware_type())
            .field("packet_type", &self.packet_type())
            .field("address", &self.address());
        if let Some(net_mask) = &self.net_mask {
            s.field("net_mask", net_mask);
        }
        if let Some(broadcast) = &self.broadcast {
            s.field("broadcast", broadcast);
        }
        if let Some(destination) = &self.destination {
            s.field("destination", destination);
        }
        s.finish()
    }
}

#[derive(Clone)]
pub struct Ipv4 {
    interface: InterfaceAddress,
    address: Option<Ipv4Addr>,
    net_mask: Option<Ipv4Network>,
    broadcast: Option<Ipv4Addr>,
    destination: Option<Ipv4Addr>,
}

impl Ipv4 {
    #[inline]
    pub fn address(&self) -> Option<Ipv4Addr> {
        self.address
    }

    #[inline]
    pub fn net_mask(&self) -> Option<Ipv4Network> {
        self.net_mask
    }

    #[inline]
    pub fn broadcast(&self) -> Option<Ipv4Addr> {
        self.broadcast
    }

    #[inline]
    pub fn destination(&self) -> Option<Ipv4Addr> {
        self.destination
    }
}

impl Debug for Ipv4 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("Ipv4");
        if let Some(address) = &self.address {
            s.field("address", address);
        }
        if let Some(net_mask) = &self.net_mask {
            s.field("net_mask", net_mask);
        }
        if let Some(broadcast) = &self.broadcast {
            s.field("broadcast", broadcast);
        }
        if let Some(destination) = &self.destination {
            s.field("destination", destination);
        }
        s.finish()
    }
}

#[derive(Clone)]
pub struct Ipv6 {
    interface: InterfaceAddress,
    address: Option<Ipv6Addr>,
    net_mask: Option<Ipv6Network>,
    broadcast: Option<Ipv6Addr>,
    destination: Option<Ipv6Addr>,
}

impl Ipv6 {
    #[inline]
    pub fn address(&self) -> Option<Ipv6Addr> {
        self.address
    }

    #[inline]
    pub fn net_mask(&self) -> Option<Ipv6Network> {
        self.net_mask
    }

    #[inline]
    pub fn broadcast(&self) -> Option<Ipv6Addr> {
        self.broadcast
    }

    #[inline]
    pub fn destination(&self) -> Option<Ipv6Addr> {
        self.destination
    }
}

impl Debug for Ipv6 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("Ipv6");
        if let Some(address) = &self.address {
            s.field("address", address);
        }
        if let Some(net_mask) = &self.net_mask {
            s.field("net_mask", net_mask);
        }
        if let Some(broadcast) = &self.broadcast {
            s.field("broadcast", broadcast);
        }
        if let Some(destination) = &self.destination {
            s.field("destination", destination);
        }
        s.finish()
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
