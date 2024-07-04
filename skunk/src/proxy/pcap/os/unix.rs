use std::{
    ffi::CStr,
    fmt::Debug,
    io::Error,
    mem::MaybeUninit,
    net::{
        Ipv4Addr,
        Ipv6Addr,
    },
    os::fd::{
        AsRawFd,
        FromRawFd,
        OwnedFd,
        RawFd,
    },
    sync::Arc,
};

use smallvec::SmallVec;
use tokio::io::{
    unix::AsyncFd,
    Interest,
    ReadBuf,
};

use crate::{
    protocol::inet::MacAddress,
    proxy::pcap::{
        interface::{
            Interface,
            InterfaceInner,
            Ipv4,
            Ipv6,
            Link,
        },
        socket::Mode,
    },
};

pub fn list_interfaces() -> Result<Vec<Interface>, Error> {
    let mut ifaddrs = IfAddrs::get()?;
    let mut addresses = ifaddrs.collect::<Vec<_>>();

    addresses.sort_by(|a, b| a.ifa_name().cmp(b.ifa_name()));
    let n = addresses.len();
    let mut it = addresses.into_iter();

    let Some(mut prev) = it.next()
    else {
        return Ok(vec![]);
    };
    let mut buf = Vec::with_capacity(n);
    buf.push(prev);

    let mut interfaces = Vec::with_capacity(n);

    while let Some(next) = it.next() {
        if next.ifa_name() != prev.ifa_name() {
            interfaces.extend(interface_from_ifaddrs(std::mem::take(&mut buf)));
            prev = next;
        }
        buf.push(next);
    }

    if !buf.is_empty() {
        // buf is empty if there was only 1 interface to begin with.
        interfaces.extend(interface_from_ifaddrs(buf));
    }

    interfaces.shrink_to_fit();
    Ok(interfaces)
}

pub fn interface_from_name(name: &str) -> Result<Option<Interface>, Error> {
    let mut buf = vec![];
    let mut if_addrs = IfAddrs::get()?;
    for address in &mut if_addrs {
        if let Ok(if_name) = address.ifa_name().to_str() {
            if if_name == name {
                buf.push(address);
            }
        }
    }
    if buf.is_empty() {
        Ok(None)
    }
    else {
        Ok(interface_from_ifaddrs(buf))
    }
}

fn interface_from_ifaddrs(addresses: Vec<&IfAddr>) -> Option<Interface> {
    let first = addresses
        .first()
        .expect("trying to create an interface with no addresses");
    for addr in &addresses[1..] {
        assert_eq!(addr.ifa_name(), first.ifa_name());
        assert_eq!(addr.0.ifa_flags, first.0.ifa_flags);
    }

    let name = first
        .ifa_name()
        .to_str()
        .expect("Invalid UTF-8 in interface name")
        .to_owned();
    let mut index = None;
    let mut link = None;
    let mut ipv4 = SmallVec::new();
    let mut ipv6 = SmallVec::new();

    for if_addr in addresses {
        if if_addr.is_family(libc::AF_PACKET as u16) {
            assert!(link.is_none(), "Interface with multiple link addresses");
            let Some(addr) = if_addr.ifa_addr()
            else {
                continue;
            };
            let addr = unsafe { addr.as_ll() };

            index = Some(addr.if_index());
            let net_mask = if_addr
                .ifa_netmask()
                .map(|a| unsafe { a.as_ll().address() });
            let ifu = if_addr.ifa_ifu().map(|a| unsafe { a.as_ll().address() });
            let (broadcast, destination) = if if_addr.0.ifa_flags & libc::IFF_BROADCAST as u32 != 0
            {
                (ifu, None)
            }
            else if if_addr.0.ifa_flags & libc::IFF_POINTOPOINT as u32 != 0 {
                (None, ifu)
            }
            else {
                (None, None)
            };
            link = Some(Link {
                address: addr.address(),
                net_mask,
                broadcast,
                destination,
            });
        }

        if if_addr.is_family(libc::AF_INET as u16) {
            let address = if_addr.ifa_addr().map(|a| unsafe { a.as_in().address() });
            let net_mask = if_addr
                .ifa_netmask()
                .map(|a| unsafe { a.as_in().address().into() });
            let ifu = if_addr.ifa_ifu().map(|a| unsafe { a.as_in().address() });
            let (broadcast, destination) = if if_addr.0.ifa_flags & libc::IFF_BROADCAST as u32 != 0
            {
                (ifu, None)
            }
            else if if_addr.0.ifa_flags & libc::IFF_POINTOPOINT as u32 != 0 {
                (None, ifu)
            }
            else {
                (None, None)
            };
            ipv4.push(Ipv4 {
                address,
                net_mask,
                broadcast,
                destination,
            });
        }

        if if_addr.is_family(libc::AF_INET6 as u16) {
            let address = if_addr.ifa_addr().map(|a| unsafe { a.as_in6().address() });
            let net_mask = if_addr
                .ifa_netmask()
                .map(|a| unsafe { a.as_in6().address().into() });
            let ifu = if_addr.ifa_ifu().map(|a| unsafe { a.as_in6().address() });
            let (broadcast, destination) = if if_addr.0.ifa_flags & libc::IFF_BROADCAST as u32 != 0
            {
                (ifu, None)
            }
            else if if_addr.0.ifa_flags & libc::IFF_POINTOPOINT as u32 != 0 {
                (None, ifu)
            }
            else {
                (None, None)
            };
            ipv6.push(Ipv6 {
                address,
                net_mask,
                broadcast,
                destination,
            });
        }
    }

    link.map(|link| {
        Interface {
            inner: Arc::new(InterfaceInner {
                name,
                index: index.unwrap(),
                link,
                ipv4,
                ipv6,
            }),
        }
    })
}

/// Non-async raw packet socket
#[derive(Debug)]
struct SyncSocket {
    fd: OwnedFd,
}

impl AsRawFd for SyncSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl SyncSocket {
    pub fn open(interface: &Interface, mode: Mode) -> Result<Self, Error> {
        let sock_type = match mode {
            Mode::Raw => libc::SOCK_RAW,
            Mode::LinuxSll => libc::SOCK_DGRAM,
        };

        // [this][1] example code uses `htons`, so we need to convert to u16 and then
        // network endian.
        //
        // [1]: https://stackoverflow.com/questions/54056426/af-packet-and-ethernet
        const ETH_P_ALL: u16 = (libc::ETH_P_ALL as u16).to_be();

        let sock = unsafe {
            libc::socket(
                libc::AF_PACKET,
                sock_type | libc::SOCK_NONBLOCK,
                ETH_P_ALL.into(),
            )
        };

        if sock == -1 {
            return Err(Error::last_os_error());
        }

        // todo: do we want to use `setsockopt(socket, BindToDevice, b"eth0")` instead?
        let mut bind_addr = libc::sockaddr_ll {
            sll_family: libc::AF_PACKET as u16,
            sll_protocol: ETH_P_ALL,
            sll_ifindex: interface.inner.index as i32,
            sll_hatype: 1,
            sll_pkttype: 0,
            sll_halen: 6,
            sll_addr: Default::default(),
        };
        bind_addr.sll_addr[0..6].copy_from_slice(&interface.link().address.0);

        let res = unsafe {
            libc::bind(
                sock,
                &bind_addr as *const libc::sockaddr_ll as *const libc::sockaddr,
                std::mem::size_of_val(&bind_addr) as u32,
            )
        };

        if res != 0 {
            return Err(Error::last_os_error());
        }

        let fd = unsafe { OwnedFd::from_raw_fd(sock) };

        Ok(Self { fd })
    }

    pub fn receive(&self, buf: &mut [MaybeUninit<u8>]) -> Result<usize, Error> {
        let res = unsafe {
            // `MsgFlags::MSG_TRUNC` tells the kernel to return the real packet length, even
            // if the buffer is not large enough.
            libc::recv(
                self.fd.as_raw_fd(),
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
                libc::MSG_TRUNC,
            )
        };

        if res == -1 {
            Err(Error::last_os_error())
        }
        else {
            Ok(res as usize)
        }
    }

    pub fn send(&self, buf: &[u8]) -> Result<usize, Error> {
        let res = unsafe {
            libc::send(
                self.fd.as_raw_fd(),
                buf.as_ptr() as *const libc::c_void,
                buf.len(),
                0,
            )
        };

        if res == -1 {
            Err(Error::last_os_error())
        }
        else {
            Ok(res as usize)
        }
    }
}

/// Async raw packet socket
#[derive(Debug)]
pub struct Socket {
    socket: AsyncFd<SyncSocket>,
}

impl Socket {
    pub fn open(interface: &Interface, mode: Mode) -> Result<Self, Error> {
        let socket = SyncSocket::open(interface, mode)?;
        let socket = AsyncFd::with_interest(socket, Interest::READABLE | Interest::WRITABLE)?;
        Ok(Self { socket })
    }

    pub async fn receive(&self, buf: &mut ReadBuf<'_>) -> Result<usize, Error> {
        self.socket
            .async_io(Interest::READABLE, |socket| {
                unsafe {
                    let filled = buf.filled().len();
                    let buf_size = buf.unfilled_mut().len();
                    let packet_length = socket.receive(buf.unfilled_mut())?;
                    let n_read = std::cmp::min(buf_size, packet_length);
                    buf.assume_init(filled + n_read);
                    buf.set_filled(filled + n_read);
                    Ok(packet_length)
                }
            })
            .await
    }

    pub async fn send(&self, buf: &[u8]) -> Result<(), Error> {
        let bytes_sent = self
            .socket
            .async_io(Interest::WRITABLE, |socket| socket.send(buf))
            .await?;
        if bytes_sent < buf.len() {
            tracing::warn!(buf_len = buf.len(), bytes_sent, "sent truncated packet");
        }
        Ok(())
    }
}

impl AsRawFd for Socket {
    fn as_raw_fd(&self) -> RawFd {
        self.socket.as_raw_fd()
    }
}

struct IfAddrs {
    first: *mut libc::ifaddrs,
    next: *mut libc::ifaddrs,
}

impl IfAddrs {
    fn get() -> Result<Self, Error> {
        let mut ifa = std::ptr::null_mut::<libc::ifaddrs>();
        let res = unsafe { libc::getifaddrs(&mut ifa as *mut *mut libc::ifaddrs) };
        if res == 0 {
            Ok(Self {
                first: ifa,
                next: ifa,
            })
        }
        else {
            Err(Error::last_os_error())
        }
    }
}

impl Drop for IfAddrs {
    fn drop(&mut self) {
        if !self.first.is_null() {
            unsafe {
                libc::freeifaddrs(self.first);
            }
        }
    }
}

impl<'a> Iterator for &'a mut IfAddrs {
    type Item = &'a IfAddr;

    fn next(&mut self) -> Option<Self::Item> {
        let ifa = self.next;
        if ifa.is_null() {
            None
        }
        else {
            unsafe {
                self.next = (*ifa).ifa_next;
                Some(&*(ifa as *const IfAddr))
            }
        }
    }
}

#[repr(transparent)]
struct IfAddr(libc::ifaddrs);

impl IfAddr {
    fn ifa_name(&self) -> &'_ CStr {
        unsafe { CStr::from_ptr(self.0.ifa_name) }
    }

    fn ifa_addr(&self) -> Option<&'_ Sockaddr> {
        if self.0.ifa_addr.is_null() {
            None
        }
        else {
            unsafe { Some(&*(self.0.ifa_addr as *const Sockaddr)) }
        }
    }

    fn ifa_netmask(&self) -> Option<&'_ Sockaddr> {
        if self.0.ifa_netmask.is_null() {
            None
        }
        else {
            unsafe { Some(&*(self.0.ifa_netmask as *const Sockaddr)) }
        }
    }

    fn ifa_ifu(&self) -> Option<&'_ Sockaddr> {
        if self.0.ifa_ifu.is_null() {
            None
        }
        else {
            unsafe { Some(&*(self.0.ifa_ifu as *const Sockaddr)) }
        }
    }

    fn is_family(&self, family: libc::sa_family_t) -> bool {
        self.ifa_addr().map_or(true, |a| a.sa_family() == family)
            && self.ifa_netmask().map_or(true, |a| a.sa_family() == family)
            && self.ifa_ifu().map_or(true, |a| a.sa_family() == family)
    }
}

#[repr(transparent)]
struct Sockaddr(libc::sockaddr);

impl Sockaddr {
    fn sa_family(&self) -> libc::sa_family_t {
        self.0.sa_family
    }

    unsafe fn as_ll(&self) -> &'_ SockaddrLl {
        &*(self as *const Self as *const SockaddrLl)
    }

    unsafe fn as_in(&self) -> &'_ SockaddrIn {
        &*(self as *const Self as *const SockaddrIn)
    }

    unsafe fn as_in6(&self) -> &'_ SockaddrIn6 {
        &*(self as *const Self as *const SockaddrIn6)
    }
}

#[repr(transparent)]
struct SockaddrLl(libc::sockaddr_ll);

impl SockaddrLl {
    fn if_index(&self) -> u32 {
        self.0.sll_ifindex.try_into().unwrap()
    }

    fn address(&self) -> MacAddress {
        let mut addr = MacAddress::default();
        addr.0.copy_from_slice(&self.0.sll_addr[..6]);
        addr
    }
}

impl Debug for SockaddrLl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SockaddrLl")
            .field("sll_family", &self.0.sll_family)
            .field("sll_protocol", &self.0.sll_protocol)
            .field("sll_ifindex", &self.0.sll_ifindex)
            .field("sll_hatype", &self.0.sll_hatype)
            .field("sll_pkttype", &self.0.sll_pkttype)
            .field("sll_halen", &self.0.sll_halen)
            .field("sll_addr", &self.0.sll_addr)
            .finish()
    }
}

#[repr(transparent)]
struct SockaddrIn(libc::sockaddr_in);

impl SockaddrIn {
    fn address(&self) -> Ipv4Addr {
        Ipv4Addr::from(self.0.sin_addr.s_addr)
    }
}

#[repr(transparent)]
struct SockaddrIn6(libc::sockaddr_in6);

impl SockaddrIn6 {
    fn address(&self) -> Ipv6Addr {
        Ipv6Addr::from(self.0.sin6_addr.s6_addr)
    }
}
