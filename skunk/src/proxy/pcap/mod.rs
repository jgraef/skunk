pub mod ap;
pub mod arp;
pub mod dhcp;
pub mod packet;
pub mod vnet;

use std::{
    fmt::{
        Debug,
        Display,
    },
    sync::Arc,
};

use etherparse::TransportSlice;
use packet::NetworkPacket;
use tokio_util::sync::CancellationToken;

use self::packet::PacketSocket;

// todo: remove?
#[derive(Debug, thiserror::Error)]
#[error("pcap error")]
pub enum Error {
    Io(#[from] std::io::Error),

    Packet(#[from] self::packet::Error),
}

impl From<nix::Error> for Error {
    fn from(e: nix::Error) -> Self {
        Self::Io(e.into())
    }
}

#[derive(Clone)]
pub struct Interface {
    inner: Arc<nix::ifaddrs::InterfaceAddress>,
}

impl Interface {
    pub fn name(&self) -> &str {
        &self.inner.interface_name
    }

    pub fn enumerate() -> Result<InterfaceIter, std::io::Error> {
        Ok(InterfaceIter {
            inner: nix::ifaddrs::getifaddrs()?,
        })
    }

    pub fn from_name(name: &str) -> Option<Interface> {
        Self::enumerate()
            .ok()?
            .find(|interface| interface.name() == name)
    }

    pub fn mac_address(&self) -> MacAddress {
        MacAddress(
            self.link_addr()
                .addr()
                .expect("interface has no MAC address"),
        )
    }

    fn link_addr(&self) -> &nix::sys::socket::LinkAddr {
        self.inner
            .address
            .as_ref()
            .expect("interface address without address")
            .as_link_addr()
            .expect("interface address is not link-layer")
    }
}

impl Debug for Interface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("Interface");
        s.field("name", &self.inner.interface_name);
        if let Some(address) = &self.inner.address {
            s.field("address", &DebugSockaddr(&address));
        }
        if let Some(netmask) = &self.inner.address {
            s.field("netmask", &DebugSockaddr(&netmask));
        }
        if let Some(broadcast) = &self.inner.address {
            s.field("broadcast", &DebugSockaddr(&broadcast));
        }
        if let Some(destination) = &self.inner.address {
            s.field("destination", &DebugSockaddr(&destination));
        }
        s.finish()
    }
}

struct DebugSockaddr<'a>(&'a nix::sys::socket::SockaddrStorage);

impl<'a> Debug for DebugSockaddr<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(link_addr) = self.0.as_link_addr() {
            Debug::fmt(link_addr, f)
        }
        else if let Some(in_addr) = self.0.as_sockaddr_in() {
            Debug::fmt(in_addr, f)
        }
        else if let Some(in_addr) = self.0.as_sockaddr_in6() {
            Debug::fmt(in_addr, f)
        }
        else {
            f.debug_tuple("Other").finish()
        }
    }
}

pub struct InterfaceIter {
    inner: nix::ifaddrs::InterfaceAddressIterator,
}

impl Iterator for InterfaceIter {
    type Item = Interface;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(interface) = self.inner.next() {
            if !interface
                .flags
                .contains(nix::net::if_::InterfaceFlags::IFF_LOOPBACK)
            {
                if let Some(address) = &interface.address {
                    if let Some(address) = address.as_link_addr() {
                        if address.protocol() == 0 && address.hatype() == 1 {
                            return Some(Interface {
                                inner: Arc::new(interface),
                            });
                        }
                    }
                }
            }
        }
        None
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MacAddress(pub [u8; 6]);

impl Display for MacAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

impl Debug for MacAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MacAddress({self})")
    }
}

impl From<[u8; 6]> for MacAddress {
    fn from(value: [u8; 6]) -> Self {
        Self(value)
    }
}

impl<'a> TryFrom<&'a [u8]> for MacAddress {
    type Error = <[u8; 6] as TryFrom<&'a [u8]>>::Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        value.try_into().map(Self)
    }
}

pub async fn run(interface: &Interface, shutdown: CancellationToken) -> Result<(), Error> {
    //let mut packets = PacketStream::open(interface)?;
    let (mut reader, _sender) = PacketSocket::open(interface)?.pair();

    loop {
        let packet = tokio::select! {
            result = reader.next() => result?,
            _ = shutdown.cancelled() => break,
        };

        println!("link:");
        println!("  source:      {}", packet.source_mac_address());
        println!("  destination: {}", packet.destination_mac_address());

        if let NetworkPacket::Ip { ipv4, transport } = packet.network {
            let ipv4_header = ipv4.header();

            println!("ipv4:");
            println!("  source:      {}", ipv4_header.source_addr());
            println!("  destination: {}", ipv4_header.destination_addr());

            match transport {
                TransportSlice::Udp(udp) => {
                    println!("udp:");
                    println!("  source:      {}", udp.source_port());
                    println!("  destination: {}", udp.destination_port());
                    println!("  payload:     {} bytes", udp.payload().len());
                }
                TransportSlice::Tcp(tcp) => {
                    println!("tcp:");
                    println!("  source:      {}", tcp.source_port());
                    println!("  destination: {}", tcp.destination_port());
                    println!("  payload:     {} bytes", tcp.payload().len());
                }
                _ => {}
            }
        }

        println!();
    }

    Ok(())
}
