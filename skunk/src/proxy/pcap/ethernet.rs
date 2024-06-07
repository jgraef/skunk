use std::fmt::{
    Debug,
    Display,
};

use super::MacAddress;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EthernetII<P> {
    pub destination: MacAddress,
    pub source: MacAddress,
    pub ether_type: EtherType,
    pub payload: P,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct EtherType(pub u16);

macro_rules! ether_type_impl {
    {$($const:ident => $number:expr;)*} => {
        impl EtherType {
            $(
                pub const $const: EtherType = Self($number);
            )*

            const fn const_name(&self) -> Option<&'static str> {
                match self.0 {
                    $(
                        $number => Some(stringify!($const)),
                    )*
                    _ => None,
                }
            }
        }
    };
}

ether_type_impl! {
    IPV4 => 0x0800;
    IPV6 => 0x86dd;
    ARP => 0x0806;
    WAKE_ON_LAN => 0x0842;
    VLAN_TAGGED_FRAME => 0x8100;
    PROVIDER_BRIDGING => 0x88A8;
    VLAN_DOUBLE_TAGGED_FRAME => 0x9100;
}

impl From<u16> for EtherType {
    fn from(value: u16) -> Self {
        Self(value)
    }
}

impl From<EtherType> for u16 {
    fn from(value: EtherType) -> Self {
        value.0
    }
}

impl Display for EtherType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Debug for EtherType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(name) = self.const_name() {
            write!(f, "EtherType::{}({})", name, self.0)
        }
        else {
            write!(f, "EtherType({})", self.0)
        }
    }
}
