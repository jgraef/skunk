use std::{
    convert::Infallible,
    fmt::Debug,
};

use byst::{
    endianness::NetworkEndian,
    io::read::{
        End,
        Read,
    },
    read,
};
use smallvec::SmallVec;

use super::{
    arp,
    ipv4,
    mac_address::MacAddress,
    vlan::VlanTag,
};
use crate::util::network_enum;

/// Max payload size for ethernet frames.
pub const MTU: usize = 1500;

/// Max frame size for ethernet frames.
pub const MAX_FRAME_SIZE: usize = 1522;

#[derive(Debug, thiserror::Error)]
#[error("Invalid ethernet packet")]
pub enum InvalidPacket {
    #[error("Frame is incomplete")]
    Incomplete(#[from] End),

    Payload(#[from] PayloadError),
}

impl From<Infallible> for InvalidPacket {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}

/// An Ethernet II header.
#[derive(Clone, Debug)]
pub struct Header {
    pub destination: MacAddress,
    pub source: MacAddress,
    pub vlan_tags: SmallVec<[VlanTag; 1]>,
    pub ether_type: EtherType,
}

impl<R> Read<R, ()> for Header
where
    MacAddress: Read<R, (), Error = End>,
    VlanTag: Read<R, (), Error = End>,
    EtherType: Read<R, (), Error = End>,
{
    type Error = InvalidPacket;

    fn read(reader: &mut R, _params: ()) -> Result<Self, Self::Error> {
        let destination = read!(reader)?;
        let source = read!(reader)?;

        let mut vlan_tags = SmallVec::new();
        let mut ether_type = read!(reader)?;

        while ether_type == EtherType::VLAN_TAGGED {
            ether_type = read!(reader)?;
            vlan_tags.push(read!(reader)?);
        }

        Ok(Self {
            destination,
            source,
            vlan_tags,
            ether_type,
        })
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
#[derive(Clone, Debug)]
pub struct Packet<P = AnyProtocol> {
    pub header: Header,
    pub payload: P,
    pub frame_check_sequence: FrameCheckSequence,
}

impl<R, P> Read<R, ()> for Packet<P>
where
    Header: Read<R, (), Error = InvalidPacket>,
    P: Read<R, EtherType, Error = PayloadError>,
    FrameCheckSequence: Read<R, (), Error = Infallible>,
{
    type Error = InvalidPacket;

    fn read(reader: &mut R, _params: ()) -> Result<Self, Self::Error> {
        let header = read!(reader => Header)?;

        let payload = read!(reader => P; header.ether_type)?;

        /*let remaining = reader.remaining();
        let frame_check_sequence = if remaining > 4 {
            reader.set_position(reader.position() + remaining - 4);
            FrameCheckSequence::Present(read!(reader => u32; NetworkEndian)?)
        }
        else {
            FrameCheckSequence::Absent
        };*/
        let frame_check_sequence = read!(reader)?;

        Ok(Self {
            header,
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
#[derive(Clone, Copy, PartialEq, Eq, Read)]
pub struct EtherType(#[byst(network)] pub u16);

network_enum! {
    for EtherType

    /// Internet protocol version 4
    IPV4 => 0x0800;

    /// Address resolution protocol
    ARP => 0x0806;

    /// Wake-on-LAN
    WAKE_ON_LAN => 0x0842;

    /// VLAN-tagged frame (IEEE 802.1Q)
    VLAN_TAGGED => 0x8100;

    /// Internet protocol version 6
    IPV6 => 0x86dd;
}

impl Debug for EtherType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(name) = self.name() {
            write!(f, "EtherType::{name}(0x{:04x})", self.0)
        }
        else {
            write!(f, "EtherType(0x{:04x})", self.0)
        }
    }
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

#[derive(Clone, Debug)]
pub enum AnyProtocol {
    Arp(arp::Packet),
    Ipv4(ipv4::Packet),
    Unknown,
}

impl<R> Read<R, EtherType> for AnyProtocol
where
    arp::Packet: Read<R, ()>,
    PayloadError: From<<arp::Packet as Read<R, ()>>::Error>,
    ipv4::Packet: Read<R, (), Error = ipv4::InvalidPacket>,
    PayloadError: From<<ipv4::Packet as Read<R, ()>>::Error>,
{
    type Error = PayloadError;

    fn read(reader: &mut R, ether_type: EtherType) -> Result<Self, Self::Error> {
        Ok(match ether_type {
            EtherType::ARP => Self::Arp(read!(reader => arp::Packet)?),
            EtherType::IPV4 => Self::Ipv4(read!(reader => ipv4::Packet)?),
            _ => Self::Unknown,
        })
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Payload error")]
pub enum PayloadError {
    Arp(#[from] arp::InvalidPacket),
    Ipv4(#[from] ipv4::InvalidPacket),
}
