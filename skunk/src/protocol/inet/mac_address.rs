use std::fmt::{
    Debug,
    Display,
};

use byst::io::{
    Read,
    Write,
};

/// A MAC address[1]
///
/// [1]: https://en.wikipedia.org/wiki/MAC_address
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Read, Write)]
pub struct MacAddress(pub [u8; 6]);

impl MacAddress {
    pub const BROADCAST: MacAddress = MacAddress([0xff; 6]);
    pub const UNSPECIFIED: MacAddress = MacAddress([0; 6]);

    #[inline]
    pub fn with_oui(&self, oui: [u8; 3]) -> Self {
        Self([oui[0], oui[1], oui[2], self.0[3], self.0[4], self.0[5]])
    }

    #[inline]
    pub fn with_nic(&self, nic: [u8; 3]) -> Self {
        Self([self.0[0], self.0[1], self.0[2], nic[0], nic[1], nic[2]])
    }

    #[inline]
    pub fn is_broadcast(&self) -> bool {
        self == &Self::BROADCAST
    }

    #[inline]
    pub fn is_unspecified(&self) -> bool {
        self == &Self::UNSPECIFIED
    }

    #[inline]
    pub fn is_universal(&self) -> bool {
        self.0[0] & 2 == 0
    }

    #[inline]
    pub fn is_local(&self) -> bool {
        self.0[0] & 2 != 0
    }

    #[inline]
    pub fn is_unicast(&self) -> bool {
        self.0[0] & 1 == 0
    }

    #[inline]
    pub fn is_multicast(&self) -> bool {
        self.0[0] & 1 != 0
    }
}

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
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}

impl From<[u8; 6]> for MacAddress {
    #[inline]
    fn from(value: [u8; 6]) -> Self {
        Self(value)
    }
}

impl From<MacAddress> for [u8; 6] {
    #[inline]
    fn from(value: MacAddress) -> Self {
        value.0
    }
}

impl<'a> TryFrom<&'a [u8]> for MacAddress {
    type Error = <[u8; 6] as TryFrom<&'a [u8]>>::Error;

    #[inline]
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        value.try_into().map(Self)
    }
}
