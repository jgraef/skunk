/// ARP protocol implementation.
///
/// # References
///
/// - [Address Resolution Protocol (ARP) Parameters](https://www.iana.org/assignments/arp-parameters/arp-parameters.xhtml)
use std::{
    io::Write,
    net::Ipv4Addr,
};

use byteorder::{
    NetworkEndian,
    WriteBytesExt,
};
pub use etherparse::{
    ArpHardwareId as HardwareType,
    EtherType as ProtocolType,
};

use super::MacAddress;
use crate::util::io::SliceReader;

#[derive(Debug, thiserror::Error)]
#[error("arp error")]
pub enum Error {
    Decode(#[from] DecodeError),
}

#[derive(Debug, thiserror::Error)]
#[error("arp decode")]
pub enum DecodeError {
    Io(#[from] std::io::Error),

    InvalidOperation(#[from] InvalidOperation),
}

const FIXED_SIZE: usize = 8;

#[derive(Clone, Debug)]
pub struct ArpSlice<'a> {
    hardware_type: HardwareType,
    protocol_type: ProtocolType,
    hardware_address_length: u8,
    protocol_address_length: u8,
    operation: Operation,
    sender_hardware_address: &'a [u8],
    sender_protocol_address: &'a [u8],
    target_hardware_address: &'a [u8],
    target_protocol_address: &'a [u8],
}

impl<'a> ArpSlice<'a> {
    pub fn from_bytes(bytes: &'a [u8]) -> Result<Self, DecodeError> {
        let mut reader = SliceReader::new(bytes);

        let hardware_type = HardwareType(reader.read_u16::<NetworkEndian>()?);
        let protocol_type = ProtocolType(reader.read_u16::<NetworkEndian>()?);
        let hardware_address_length = reader.read_u8()?;
        let protocol_address_length = reader.read_u8()?;
        let operation = Operation::try_from(reader.read_u16::<NetworkEndian>()?)?;
        let sender_hardware_address = reader.read_subslice(hardware_address_length)?;
        let sender_protocol_address = reader.read_subslice(protocol_address_length)?;
        let target_hardware_address = reader.read_subslice(hardware_address_length)?;
        let target_protocol_address = reader.read_subslice(protocol_address_length)?;

        Ok(Self {
            hardware_type,
            protocol_type,
            hardware_address_length,
            protocol_address_length,
            operation,
            sender_hardware_address,
            sender_protocol_address,
            target_hardware_address,
            target_protocol_address,
        })
    }

    pub fn len(&self) -> usize {
        FIXED_SIZE
            + usize::from(self.hardware_address_length) * 2
            + usize::from(self.protocol_address_length) * 2
    }

    pub fn write(&self, mut writer: impl Write) -> Result<(), std::io::Error> {
        writer.write_u16::<NetworkEndian>(self.hardware_type.0)?;
        writer.write_u16::<NetworkEndian>(self.protocol_type.0)?;
        writer.write_u8(self.hardware_address_length)?;
        writer.write_u8(self.protocol_address_length)?;
        writer.write_u16::<NetworkEndian>(self.operation.into())?;
        writer.write_all(self.sender_hardware_address)?;
        writer.write_all(self.sender_protocol_address)?;
        writer.write_all(self.target_hardware_address)?;
        writer.write_all(self.target_protocol_address)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ArpBuf<H, P> {
    pub hardware_type: HardwareType,
    pub protocol_type: ProtocolType,
    pub operation: Operation,
    pub sender_hardware_address: H,
    pub sender_protocol_address: P,
    pub target_hardware_address: H,
    pub target_protocol_address: P,
}

impl<H: HardwareAddress, P: ProtocolAddress> ArpBuf<H, P> {
    pub const fn len(&self) -> usize {
        FIXED_SIZE + H::SIZE * 2 + P::SIZE * 2
    }

    pub fn write(&self, mut writer: impl Write) -> Result<(), std::io::Error> {
        writer.write_u16::<NetworkEndian>(self.hardware_type.0)?;
        writer.write_u16::<NetworkEndian>(self.protocol_type.0)?;
        writer.write_u8(H::SIZE as u8)?;
        writer.write_u8(P::SIZE as u8)?;
        writer.write_u16::<NetworkEndian>(self.operation.into())?;
        self.sender_hardware_address.write(&mut writer)?;
        self.sender_protocol_address.write(&mut writer)?;
        self.target_hardware_address.write(&mut writer)?;
        self.target_protocol_address.write(&mut writer)?;
        Ok(())
    }
}

macro_rules! address_trait {
    ($name:ident) => {
        pub trait $name {
            const SIZE: usize;
            fn write(&self, writer: impl Write) -> Result<(), std::io::Error>;
        }

        impl<const N: usize> $name for [u8; N] {
            const SIZE: usize = N;

            fn write(&self, mut writer: impl Write) -> Result<(), std::io::Error> {
                writer.write_all(self)
            }
        }
    };
}

address_trait!(HardwareAddress);

impl HardwareAddress for MacAddress {
    const SIZE: usize = 6;

    fn write(&self, mut writer: impl Write) -> Result<(), std::io::Error> {
        writer.write_all(&self.0)
    }
}

address_trait!(ProtocolAddress);

impl ProtocolAddress for Ipv4Addr {
    const SIZE: usize = 4;

    fn write(&self, mut writer: impl Write) -> Result<(), std::io::Error> {
        writer.write_all(&self.octets())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Request,
    Reply,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid operation: {value}")]
pub struct InvalidOperation {
    value: u16,
}

impl TryFrom<u16> for Operation {
    type Error = InvalidOperation;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Reply),
            2 => Ok(Self::Reply),
            _ => Err(InvalidOperation { value }),
        }
    }
}

impl From<Operation> for u16 {
    fn from(value: Operation) -> Self {
        match value {
            Operation::Request => 1,
            Operation::Reply => 2,
        }
    }
}
