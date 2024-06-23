use std::{
    convert::Infallible,
    fmt::Debug,
};

use byst::{
    endianness::NetworkEndian,
    io::{
        read,
        BufReader,
        FailedPartially,
        Limit,
        LimitError,
        Read,
        Reader,
        ReaderExt,
    },
    Bytes,
};
use smallvec::SmallVec;

use super::{
    arp,
    ipv4,
    mac_address::MacAddress,
};
use crate::util::{
    network_enum,
    CrcExt,
};

/// Max payload size for ethernet frames.
pub const MTU: usize = 1500;

/// Max frame size for ethernet frames.
pub const MAX_FRAME_SIZE: usize = 1522;

/// An Ethernet II header.
#[derive(Clone, Debug)]
pub struct Header {
    pub destination: MacAddress,
    pub source: MacAddress,
    pub vlan_tags: SmallVec<[VlanTag; 1]>,
    pub ether_type: EtherType,
}

impl<R: Reader> Read<R, ()> for Header {
    type Error = InvalidHeader<R::Error>;

    fn read(reader: &mut R, _params: ()) -> Result<Self, Self::Error> {
        let destination = reader.read()?;
        let source = reader.read()?;

        let mut vlan_tags = SmallVec::new();
        let mut ether_type = reader.read()?;

        // If we find at least one IEEE 802.1ad VLAN tag, we also expect a final VLAN
        // tag according to IEEE 802.1Q
        let expect_vlan_tag_q = ether_type == EtherType::VLAN_TAGGED_QINQ;

        // First we read VLAN tags according to IEEE 802.1ad
        while ether_type == EtherType::VLAN_TAGGED_QINQ {
            vlan_tags.push(reader.read()?);
            ether_type = reader.read()?;
        }

        // Then we read the final IEEE 802.1Q VLAN tag
        if expect_vlan_tag_q {
            if ether_type == EtherType::VLAN_TAGGED {
                vlan_tags.push(reader.read()?);
                ether_type = reader.read()?;
            }
            else {
                return Err(InvalidHeader::Expected8021QTag);
            }
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
pub struct Frame<P = Bytes> {
    pub header: Header,
    pub payload: P,
    pub frame_check_sequence: FrameCheckSequence,
}

impl<R: BufReader, P, E> Read<R, ()> for Frame<P>
where
    P: for<'r> Read<Limit<&'r mut R>, EtherType, Error = E>,
    R::Error: FailedPartially,
{
    type Error = InvalidFrame<R::Error, E>;

    fn read(reader: &mut R, _params: ()) -> Result<Self, Self::Error> {
        let fcs_present = {
            // Compute CRC32 for this frame. if it's the expected value, the FCS is present.
            let view = reader.view(reader.remaining()).unwrap();
            FCS_CRC32.for_buf(view) == FCS_CRC32.residue
        };

        // Read the header
        let header: Header = reader.read()?;

        // We currently support only Ethernet II
        if !header.ether_type.is_ethernet2() {
            return Err(InvalidFrame::NotEthernet2 {
                ether_type: header.ether_type,
            });
        }

        // Read the payload, with at most `payload_size` bytes.
        let payload_size = if fcs_present {
            reader.remaining().saturating_sub(4)
        }
        else {
            reader.remaining()
        };
        let mut limit = reader.limit(payload_size);
        let payload = limit
            .read_with(header.ether_type)
            .map_err(InvalidFrame::Payload)?;

        // Skip over padding
        let _ = limit.skip_remaining();

        let frame_check_sequence = fcs_present
            .then(|| reader.read_with(NetworkEndian).ok())
            .flatten()
            .into();

        Ok(Self {
            header,
            payload,
            frame_check_sequence,
        })
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid Ethernet II header")]
pub enum InvalidHeader<R> {
    Read(#[from] R),

    #[error("Expected a final IEEE 802.1Q VLAN tag")]
    Expected8021QTag,
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid ethernet packet")]
pub enum InvalidFrame<R, P = AnyPayloadError<LimitError<R>>> {
    Header(#[from] InvalidHeader<R>),
    Payload(#[source] P),
    #[error("Frame is not an Ethernet II frame: {ether_type:?}")]
    NotEthernet2 {
        ether_type: EtherType,
    },
}

impl<P> From<Infallible> for InvalidFrame<P> {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}

/// Payload type for an [`EthernetFrame`].
///
/// > EtherType is a two-octet field in an Ethernet frame. It is used to
/// > indicate which protocol is encapsulated in the payload of the frame and
/// > is used at the receiving end by the data link layer to determine how the
/// > payload is processed. The same field is also used to indicate the size of
/// > some Ethernet frames.[1]
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

    /// VLAN-tagged frame (IEEE 802.1ad)
    VLAN_TAGGED_QINQ => 0x88a8;
}

impl EtherType {
    #[inline]
    pub fn as_frame_length(&self) -> Option<u16> {
        (self.0 <= 1500).then_some(self.0)
    }

    #[inline]
    pub fn is_ethernet2(&self) -> bool {
        self.0 >= 0x0600
    }
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

impl From<Option<u32>> for FrameCheckSequence {
    #[inline]
    fn from(value: Option<u32>) -> Self {
        value.map_or(Self::Absent, Self::Present)
    }
}

#[derive(Clone, Debug)]
pub enum AnyProtocol {
    Arp(arp::Packet),
    Ipv4(ipv4::Packet),
    Unknown,
}

impl<R: Reader> Read<R, EtherType> for AnyProtocol
where
    arp::Packet: Read<R, (), Error = arp::InvalidPacket<R::Error>>,
    ipv4::Packet: Read<R, (), Error = ipv4::InvalidPacket<R::Error>>,
{
    type Error = AnyPayloadError<R::Error>;

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
pub enum AnyPayloadError<R> {
    Arp(#[from] arp::InvalidPacket<R>),
    Ipv4(#[from] ipv4::InvalidPacket<R>),
}

/// Vlan tag for ethernet frames[1]
///
/// [1]: https://en.wikipedia.org/wiki/IEEE_802.1Q
#[derive(Clone, Copy, Debug, Read, Default)]
pub struct VlanTag(#[byst(network)] pub u16);

impl VlanTag {
    #[inline]
    pub fn pcp(&self) -> u8 {
        (self.0 >> 13) as u8
    }

    pub fn with_pcp(mut self, pcp: u8) -> Self {
        self.0 = (self.0 & 0x1fff) | ((pcp as u16) << 13);
        self
    }

    #[inline]
    pub fn drop_eligible(&self) -> bool {
        self.0 & 0x1000 != 0
    }

    pub fn with_drop_eligible(mut self, drop_eligible: bool) -> Self {
        if drop_eligible {
            self.0 |= 0x1000;
        }
        else {
            self.0 &= !0xefff;
        }
        self
    }

    pub fn vlan_identifier(&self) -> u16 {
        self.0 & 0xfff
    }

    pub fn with_vlan_identifier(mut self, vlan_identifier: u16) -> Self {
        self.0 |= vlan_identifier & 0xfff;
        self
    }
}

pub const FCS_CRC32: crc::Algorithm<u32> = crc::Algorithm {
    width: 32,
    poly: 0x04c11db7,
    init: 0xffffffff,
    refin: true,
    refout: true,
    xorout: 0xffffffff,
    check: 0xcbf43926,
    residue: 0xdebb20e3,
};
