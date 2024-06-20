use bitflags::bitflags;
use byst::{
    endianness::NetworkEndian,
    io::{
        read,
        End,
        Read,
    },
};

use crate::proxy::pcap::interface::Ipv4;

#[derive(Debug, thiserror::Error)]
#[error("Invalid IPv4 packet")]
pub enum InvalidPacket {
    #[error("Packet is incomplete")]
    Incomplete(#[from] End),

    Payload(#[from] PayloadError),

    #[error("Invalid internet header length: {value}")]
    InvalidInternetHeaderLength {
        value: u8,
    },
}

#[derive(Clone, Debug)]
pub struct Header {
    pub version: u8,
    pub internet_header_length: u8,
    pub differentiated_service_code_point: u8,
    pub explicit_congestion_notification: u8,
    pub total_length: u16,
    pub identification: u16,
    pub flags: Flags,
    pub fragment_offset: u16,
    pub time_to_live: u8,
    pub protocol: u8,
    pub header_checksum: u16,
    pub source_address: Ipv4,
    pub destination_address: Ipv4,
    //pub options: Options,
}

impl<R> Read<R, ()> for Header
where
    u8: Read<R, ()>,
    InvalidPacket: From<<u8 as Read<R, ()>>::Error>,
    u16: Read<R, NetworkEndian>,
    InvalidPacket: From<<u16 as Read<R, NetworkEndian>>::Error>,
    Ipv4: Read<R, ()>,
    InvalidPacket: From<<Ipv4 as Read<R, ()>>::Error>,
{
    type Error = InvalidPacket;

    fn read(reader: &mut R, _params: ()) -> Result<Self, Self::Error> {
        let version_ihl = read!(reader => u8)?;
        let version = version_ihl >> 4;
        let internet_header_length = version_ihl & 0xf;
        if internet_header_length != 5 {
            // todo: support options
            return Err(InvalidPacket::InvalidInternetHeaderLength {
                value: internet_header_length,
            });
        }

        let dscp_ecn = read!(reader => u8)?;
        let differentiated_service_code_point = dscp_ecn >> 2;
        let explicit_congestion_notification = dscp_ecn & 3;

        let total_length = read!(reader; NetworkEndian)?;

        let identification = read!(reader; NetworkEndian)?;

        let flags_fragment_offset = read!(reader => u16; NetworkEndian)?;
        let flags = Flags::from_bits_truncate((flags_fragment_offset >> 13) as u8);
        let fragment_offset = flags_fragment_offset & 0x1fff;

        let time_to_live = read!(reader)?;

        let protocol = read!(reader)?;

        let header_checksum = read!(reader; NetworkEndian)?;

        let source_address = read!(reader)?;

        let destination_address = read!(reader)?;

        Ok(Self {
            version,
            internet_header_length,
            differentiated_service_code_point,
            explicit_congestion_notification,
            total_length,
            identification,
            flags,
            fragment_offset,
            time_to_live,
            protocol,
            header_checksum,
            source_address,
            destination_address,
            //options,
        })
    }
}

bitflags! {
    #[derive(Clone, Copy, Debug)]
    pub struct Flags: u8 {
        const RESERVED = 0b100;
        const DONT_FRAGMENT = 0b010;
        const MORE_FRAGMENTS = 0b001;
    }
}

#[derive(Clone, Debug)]
pub struct Packet<P = AnyPayload> {
    payload: P,
}

impl<R, P> Read<R, ()> for Packet<P>
where
    P: Read<R, (), Error = PayloadError>,
{
    type Error = InvalidPacket;

    fn read(reader: &mut R, _params: ()) -> Result<Self, Self::Error> {
        let _payload = read!(reader => P)?;

        todo!();
    }
}

#[derive(Clone, Debug)]
pub enum AnyPayload {}

impl<R> Read<R, ()> for AnyPayload {
    type Error = PayloadError;

    fn read(_reader: &mut R, _params: ()) -> Result<Self, Self::Error> {
        todo!();
    }
}

#[derive(Debug, thiserror::Error)]
#[error("IPv4 payload layer error")]
pub enum PayloadError {}
