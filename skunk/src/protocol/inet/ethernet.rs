use std::convert::Infallible;

use byst::{
    endianness::NetworkEndian,
    io::read::{
        End,
        Read,
        ReadIntoBuf,
    },
    read,
};
use smallvec::SmallVec;

use super::{
    mac_address::MacAddress,
    vlan::VlanTag,
};

/// Max payload size for ethernet frames.
pub const MTU: usize = 1500;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Frame is incomplete")]
    Incomplete(#[from] End),
}

impl From<Infallible> for Error {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}

/// An Ethernet II frame.
///
/// > In computer networking, an Ethernet frame is a data link layer protocol
/// > data
/// > unit and uses the underlying Ethernet physical layer transport mechanisms.
/// > In other words, a data unit on an Ethernet link transports an Ethernet
/// > frame
/// > as its payload.[1]
///
/// [1]: https://en.wikipedia.org/wiki/Ethernet_frame
pub struct EthernetFrame<Payload = AnyPayload> {
    pub destination: MacAddress,
    pub source: MacAddress,
    pub vlan_tags: SmallVec<[VlanTag; 1]>,
    pub ether_type: EtherType,
    pub payload: Payload,
    pub frame_check_sequence: FrameCheckSequence,
}

impl<R, Payload> Read<R, ()> for EthernetFrame<Payload>
where
    R: ReadIntoBuf<Error = End>,
    Payload: Read<R, EtherType, Error = Error>,
{
    type Error = Error;

    fn read(reader: &mut R, _params: ()) -> Result<Self, Self::Error> {
        let destination = read!(reader)?;
        let source = read!(reader)?;

        let mut vlan_tags = SmallVec::new();
        let mut ether_type = read!(reader)?;

        while ether_type == EtherType::VLAN_TAGGED {
            ether_type = read!(reader)?;
            vlan_tags.push(read!(reader)?);
        }

        let payload = read!(reader; ether_type)?;

        let frame_check_sequence = read!(reader)?;

        Ok(Self {
            destination,
            source,
            vlan_tags,
            ether_type,
            payload,
            frame_check_sequence,
        })
    }
}

/// Payload type for an [`EthernetFrame`].
///
/// > EtherType is a two-octet field in an Ethernet frame. It is used to
/// > indicate
/// > which protocol is encapsulated in the payload of the frame and is used at
/// > the receiving end by the data link layer to determine how the payload is
/// > processed. The same field is also used to indicate the size of some
/// > Ethernet
/// > frames.[1]
///
/// [1]: https://en.wikipedia.org/wiki/EtherType
#[derive(Clone, Copy, Debug, PartialEq, Eq, Read)]
pub struct EtherType(#[byst(network)] pub u16);

impl EtherType {
    /// Internet protocol version 4
    pub const IPV4: Self = Self(0x0800);

    /// Address resolution protocol
    pub const ARP: Self = Self(0x0806);

    /// Wake-on-LAN
    pub const WAKE_ON_LAN: Self = Self(0x0842);

    /// VLAN-tagged frame (IEEE 802.1Q)
    pub const VLAN_TAGGED: Self = Self(0x8100);

    /// Internet protocol version 6
    pub const IPV6: Self = Self(0x86dd);
}

#[derive(Clone, Copy, Debug)]
pub enum FrameCheckSequence {
    Absent,
    Present(u32),
    Calculate,
}

impl<R> Read<R, ()> for FrameCheckSequence
where
    u32: Read<R, NetworkEndian>,
{
    type Error = Infallible;

    fn read(reader: &mut R, _parameters: ()) -> Result<Self, Self::Error> {
        // todo: ideally we would want to know from the error if it happened because
        // we're at the end of the reader
        if let Ok(value) = read!(reader => u32; NetworkEndian) {
            Ok(Self::Present(value))
        }
        else {
            Ok(Self::Absent)
        }
    }
}

#[derive(Clone, Debug, Read)]
#[byst(
    params(
        name = "ether_type",
        ty = "EtherType"
    ),
    match_expr = ether_type,
    no_wild,
    error = "Error",
)]
pub enum AnyPayload {
    //#[byst(discriminant = "EtherType::ARP")]
    //Arp(ArpPacket),
    #[byst(discriminant = "_")]
    Unknown,
}
