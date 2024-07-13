use std::{
    fmt::{
        Debug,
        Display,
    },
    str::FromStr,
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

#[derive(Debug, thiserror::Error)]
#[error("Could not parse MAC address: {0}")]
pub struct MacAddressParseError(String);

impl FromStr for MacAddress {
    type Err = MacAddressParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        #[inline]
        fn hex(s: &mut &str) -> Option<u8> {
            let (a, b) = s.split_at(2);
            let a = u8::from_str_radix(a, 16).ok()?;
            *s = b;
            Some(a)
        }

        #[inline]
        fn colon(s: &mut &str) -> Option<()> {
            let (a, b) = s.split_at(1);
            *s = b;
            (a == ":").then_some(())
        }

        #[inline]
        fn parse(mut s: &str) -> Option<[u8; 6]> {
            if !s.is_ascii() || s.len() != 17 {
                return None;
            }

            let mut buf = [0u8; 6];

            buf[0] = hex(&mut s)?;
            for b in &mut buf[1..6] {
                colon(&mut s)?;
                *b = hex(&mut s)?;
            }

            Some(buf)
        }

        parse(s)
            .map(Self)
            .ok_or_else(|| MacAddressParseError(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::MacAddress;

    #[test]
    fn it_parses_a_mac_address() {
        let mac: MacAddress = "12:34:56:78:90:af".parse().unwrap();
        assert_eq!(mac, MacAddress([0x12, 0x34, 0x56, 0x78, 0x90, 0xaf]));
    }
}
