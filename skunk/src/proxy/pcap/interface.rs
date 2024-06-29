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
use smallvec::SmallVec;

use super::socket::{
    Mode,
    Receiver,
    Sender,
    Socket,
};
use crate::protocol::inet::MacAddress;

pub(super) struct InterfaceInner {
    pub(super) name: String,
    pub(super) index: u32,
    pub(super) link: Link,
    pub(super) ipv4: SmallVec<[Ipv4; 1]>,
    pub(super) ipv6: SmallVec<[Ipv6; 1]>,
}

/// A network interface.
#[derive(Clone)]
pub struct Interface {
    pub(super) inner: Arc<InterfaceInner>,
}

impl Interface {
    #[inline]
    pub fn list() -> Result<Vec<Interface>, std::io::Error> {
        super::os::list_interfaces()
    }

    #[inline]
    pub fn from_name(name: &str) -> Result<Option<Interface>, std::io::Error> {
        super::os::interface_from_name(name)
    }

    #[inline]
    pub fn link(&self) -> &Link {
        &self.inner.link
    }

    #[inline]
    pub fn ipv4(&self) -> ConfigIter<'_, Ipv4> {
        ConfigIter {
            inner: self.inner.ipv4.iter(),
        }
    }

    #[inline]
    pub fn ipv6(&self) -> ConfigIter<'_, Ipv6> {
        ConfigIter {
            inner: self.inner.ipv6.iter(),
        }
    }

    #[inline]
    pub fn name(&self) -> &str {
        &self.inner.name
    }

    #[inline]
    pub fn hardware_address(&self) -> MacAddress {
        self.inner.link.address()
    }

    pub fn if_index(&self) -> u32 {
        self.inner.index
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
    pub(super) address: MacAddress,
    pub(super) net_mask: Option<MacAddress>,
    pub(super) broadcast: Option<MacAddress>,
    pub(super) destination: Option<MacAddress>,
}

impl Link {
    #[inline]
    pub fn address(&self) -> MacAddress {
        self.address
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
        s.field("address", &self.address());
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
    pub(super) address: Option<Ipv4Addr>,
    pub(super) net_mask: Option<Ipv4Network>,
    pub(super) broadcast: Option<Ipv4Addr>,
    pub(super) destination: Option<Ipv4Addr>,
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
    pub(super) address: Option<Ipv6Addr>,
    pub(super) net_mask: Option<Ipv6Network>,
    pub(super) broadcast: Option<Ipv6Addr>,
    pub(super) destination: Option<Ipv6Addr>,
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
