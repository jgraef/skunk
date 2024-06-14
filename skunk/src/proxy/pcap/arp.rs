/// ARP protocol implementation.
///
/// # References
/// - [An Ethernet Address Resolution Protocol](https://datatracker.ietf.org/doc/html/rfc826)
/// - [Address Resolution Protocol (ARP) Parameters](https://www.iana.org/assignments/arp-parameters/arp-parameters.xhtml)
use std::{
    collections::HashMap,
    fmt::Display,
    io::Write,
    net::{
        IpAddr,
        Ipv4Addr,
        Ipv6Addr,
    },
};

pub use etherparse::{
    ArpHardwareId as HardwareType,
    EtherType as ProtocolType,
};
use futures::Future;
use byst::rw::{Cursor, Read, read};

use super::{
    packet::WritePacket,
    MacAddress,
};

#[derive(Debug, thiserror::Error)]
#[error("arp error")]
pub enum Error {
    Decode(#[from] DecodeError),
    Encode(#[from] EncodeError),
    Send(#[source] super::packet::SendError),
}

#[derive(Debug, thiserror::Error)]
#[error("arp decode error")]
pub enum DecodeError {
    Io(#[from] std::io::Error),

    InvalidOperation(#[from] InvalidOperation),
}

#[derive(Debug, thiserror::Error)]
#[error("arp encode error")]
pub enum EncodeError {
    Io(#[from] std::io::Error),
}

const FIXED_SIZE: usize = 8;

mod test {
    use super::{HardwareType, ProtocolType, Operation, Read};

    #[derive(Clone, Debug, Read)]
    pub struct ArpPacketSlice<'a> {
        pub hardware_type: HardwareType,
        pub protocol_type: ProtocolType,
        pub hardware_address_length: u8,
        pub protocol_address_length: u8,
        pub operation: Operation,
        pub sender_hardware_address: &'a [u8],
        pub sender_protocol_address: &'a [u8],
        pub target_hardware_address: &'a [u8],
        pub target_protocol_address: &'a [u8],
    }
}

fn test_read() {
    let mut cursor = Cursor::new(b"");
    //let arp = read!(cursor => ArpPacketSlice);

    todo!();
}

pub use test::ArpPacketSlice;

impl<'a> ArpPacketSlice<'a> {
    pub fn from_bytes(_bytes: &'a [u8]) -> Result<Self, DecodeError> {
        /*let mut reader = Cursor::new(bytes);

        let hardware_type = HardwareType(reader.read_u16::<NetworkEndian>()?);
        let protocol_type = ProtocolType(reader.read_u16::<NetworkEndian>()?);
        let hardware_address_length = reader.read_u8()?;
        let protocol_address_length = reader.read_u8()?;
        let operation = Operation::try_from(reader.read_u16::<NetworkEndian>()?)?;
        let sender_hardware_address = reader.read_slice(hardware_address_length)?;
        let sender_protocol_address = reader.read_slice(protocol_address_length)?;
        let target_hardware_address = reader.read_slice(hardware_address_length)?;
        let target_protocol_address = reader.read_slice(protocol_address_length)?;

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
        })*/
        todo!();
    }

    pub fn len(&self) -> usize {
        FIXED_SIZE
            + usize::from(self.hardware_address_length) * 2
            + usize::from(self.protocol_address_length) * 2
    }

    pub fn write(&self, _writer: impl Write) -> Result<(), EncodeError> {
        /*writer.write_u16::<NetworkEndian>(self.hardware_type.0)?;
        writer.write_u16::<NetworkEndian>(self.protocol_type.0)?;
        writer.write_u8(self.hardware_address_length)?;
        writer.write_u8(self.protocol_address_length)?;
        writer.write_u16::<NetworkEndian>(self.operation.into())?;
        writer.write_all(self.sender_hardware_address)?;
        writer.write_all(self.sender_protocol_address)?;
        writer.write_all(self.target_hardware_address)?;
        writer.write_all(self.target_protocol_address)?;
        Ok(())*/
        todo!();
    }
}

impl<'a> WritePacket for ArpPacketSlice<'a> {
    fn write_packet(&self, writer: impl Write) -> Result<(), super::packet::EncodeError> {
        self.write(writer).map_err(|e| {
            // right now we can still do this because arp::EncodeError is always an
            // std::io::Error.
            match e {
                EncodeError::Io(e) => e.into(),
            }
        })
    }
}

#[derive(Clone, Debug)]
pub struct ArpPacket<H, P> {
    pub hardware_type: HardwareType,
    pub protocol_type: ProtocolType,
    pub operation: Operation,
    pub sender_hardware_address: H,
    pub sender_protocol_address: P,
    pub target_hardware_address: H,
    pub target_protocol_address: P,
}

impl<H: HardwareAddress, P: ProtocolAddress> ArpPacket<H, P> {
    pub const fn len(&self) -> usize {
        FIXED_SIZE + H::SIZE * 2 + P::SIZE * 2
    }

    pub fn write(&self, _writer: impl Write) -> Result<(), EncodeError> {
        /*writer.write_u16::<NetworkEndian>(self.hardware_type.0)?;
        writer.write_u16::<NetworkEndian>(self.protocol_type.0)?;
        writer.write_u8(H::SIZE as u8)?;
        writer.write_u8(P::SIZE as u8)?;
        writer.write_u16::<NetworkEndian>(self.operation.into())?;
        self.sender_hardware_address.write(&mut writer)?;
        self.sender_protocol_address.write(&mut writer)?;
        self.target_hardware_address.write(&mut writer)?;
        self.target_protocol_address.write(&mut writer)?;
        Ok(())*/
        todo!();
    }
}

impl<H: HardwareAddress, P: ProtocolAddress> WritePacket for ArpPacket<H, P> {
    fn write_packet(&self, writer: impl Write) -> Result<(), super::packet::EncodeError> {
        self.write(writer).map_err(|e| {
            // right now we can still do this because arp::EncodeError is always an
            // std::io::Error.
            match e {
                EncodeError::Io(e) => e.into(),
            }
        })
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid ARP packet")]
pub enum InvalidPacket {
    #[error("invalid hardware type: expected {expected:?}, but got {got:?}")]
    InvalidHardwareType {
        expected: HardwareType,
        got: HardwareType,
    },

    #[error("invalid protocol type: expected {expected:?}, but got {got:?}")]
    InvalidProtocolType {
        expected: ProtocolType,
        got: ProtocolType,
    },

    #[error("invalid protocol type: expected {expected:?} or {expected2:?}, but got {got:?}")]
    InvalidProtocolType2 {
        expected: ProtocolType,
        expected2: ProtocolType,
        got: ProtocolType,
    },

    #[error("invalid hardware address length: expected {expected:?}, but got {got:?}")]
    InvalidHardwareAddressLength { expected: usize, got: u8 },

    #[error("invalid protocol address length: expected {expected:?}, but got {got:?}")]
    InvalidProtocolAddressLength { expected: usize, got: u8 },
}

macro_rules! impl_from_slice {
    ($haddr:ty, $hty:expr, $paddr:ty, $pty:expr) => {
        impl<'a> TryFrom<ArpPacketSlice<'a>> for ArpPacket<$haddr, $paddr> {
            type Error = InvalidPacket;

            fn try_from(value: ArpPacketSlice<'a>) -> Result<Self, Self::Error> {
                if value.hardware_type != $hty {
                    Err(InvalidPacket::InvalidHardwareType {
                        expected: $hty,
                        got: value.hardware_type,
                    })
                }
                else if value.protocol_type != $pty {
                    Err(InvalidPacket::InvalidProtocolType {
                        expected: $pty,
                        got: value.protocol_type,
                    })
                }
                else if value.hardware_address_length as usize
                    != <$haddr as HardwareAddress>::SIZE
                {
                    Err(InvalidPacket::InvalidHardwareAddressLength {
                        expected: <$haddr as HardwareAddress>::SIZE,
                        got: value.hardware_address_length,
                    })
                }
                else if value.protocol_address_length as usize
                    != <$paddr as ProtocolAddress>::SIZE
                {
                    Err(InvalidPacket::InvalidProtocolAddressLength {
                        expected: <$paddr as ProtocolAddress>::SIZE,
                        got: value.protocol_address_length,
                    })
                }
                else {
                    Ok(ArpPacket {
                        hardware_type: value.hardware_type,
                        protocol_type: value.protocol_type,
                        operation: value.operation,
                        sender_hardware_address: <$haddr as HardwareAddress>::from_bytes(
                            value.sender_hardware_address,
                        ),
                        sender_protocol_address: <$paddr as ProtocolAddress>::from_bytes(
                            value.sender_protocol_address,
                        ),
                        target_hardware_address: <$haddr as HardwareAddress>::from_bytes(
                            value.target_hardware_address,
                        ),
                        target_protocol_address: <$paddr as ProtocolAddress>::from_bytes(
                            value.target_protocol_address,
                        ),
                    })
                }
            }
        }
    };
}

macro_rules! impl_from_slice_any_ip {
    ($haddr:ty, $hty:expr) => {
        impl<'a> TryFrom<ArpPacketSlice<'a>> for ArpPacket<$haddr, IpAddr> {
            type Error = InvalidPacket;

            fn try_from(value: ArpPacketSlice<'a>) -> Result<Self, Self::Error> {
                if value.hardware_type != $hty {
                    Err(InvalidPacket::InvalidHardwareType {
                        expected: $hty,
                        got: value.hardware_type,
                    })
                }
                else if value.hardware_address_length as usize
                    != <$haddr as HardwareAddress>::SIZE
                {
                    Err(InvalidPacket::InvalidHardwareAddressLength {
                        expected: <$haddr as HardwareAddress>::SIZE,
                        got: value.hardware_address_length,
                    })
                }
                else if value.protocol_type == ProtocolType::IPV4 {
                    if value.protocol_address_length as usize != <Ipv4Addr as ProtocolAddress>::SIZE
                    {
                        Err(InvalidPacket::InvalidProtocolAddressLength {
                            expected: <Ipv4Addr as ProtocolAddress>::SIZE,
                            got: value.protocol_address_length,
                        })
                    }
                    else {
                        Ok(ArpPacket {
                            hardware_type: value.hardware_type,
                            protocol_type: value.protocol_type,
                            operation: value.operation,
                            sender_hardware_address: <$haddr as HardwareAddress>::from_bytes(
                                value.sender_hardware_address,
                            ),
                            sender_protocol_address: IpAddr::V4(
                                <Ipv4Addr as ProtocolAddress>::from_bytes(
                                    value.sender_protocol_address,
                                ),
                            ),
                            target_hardware_address: <$haddr as HardwareAddress>::from_bytes(
                                value.target_hardware_address,
                            ),
                            target_protocol_address: IpAddr::V4(
                                <Ipv4Addr as ProtocolAddress>::from_bytes(
                                    value.target_protocol_address,
                                ),
                            ),
                        })
                    }
                }
                else if value.protocol_type == ProtocolType::IPV6 {
                    if value.protocol_address_length as usize != <Ipv6Addr as ProtocolAddress>::SIZE
                    {
                        Err(InvalidPacket::InvalidProtocolAddressLength {
                            expected: <Ipv6Addr as ProtocolAddress>::SIZE,
                            got: value.protocol_address_length,
                        })
                    }
                    else {
                        Ok(ArpPacket {
                            hardware_type: value.hardware_type,
                            protocol_type: value.protocol_type,
                            operation: value.operation,
                            sender_hardware_address: <$haddr as HardwareAddress>::from_bytes(
                                value.sender_hardware_address,
                            ),
                            sender_protocol_address: IpAddr::V6(
                                <Ipv6Addr as ProtocolAddress>::from_bytes(
                                    value.sender_protocol_address,
                                ),
                            ),
                            target_hardware_address: <$haddr as HardwareAddress>::from_bytes(
                                value.target_hardware_address,
                            ),
                            target_protocol_address: IpAddr::V6(
                                <Ipv6Addr as ProtocolAddress>::from_bytes(
                                    value.target_protocol_address,
                                ),
                            ),
                        })
                    }
                }
                else {
                    Err(InvalidPacket::InvalidProtocolType2 {
                        expected: ProtocolType::IPV4,
                        expected2: ProtocolType::IPV6,
                        got: value.protocol_type,
                    })
                }
            }
        }
    };
}

impl_from_slice!(
    MacAddress,
    HardwareType::ETHER,
    Ipv4Addr,
    ProtocolType::IPV4
);
impl_from_slice!(
    MacAddress,
    HardwareType::ETHER,
    Ipv6Addr,
    ProtocolType::IPV6
);
impl_from_slice_any_ip!(MacAddress, HardwareType::ETHER);

macro_rules! address_trait {
    ($name:ident) => {
        pub trait $name {
            const SIZE: usize;
            fn write(&self, writer: impl Write) -> Result<(), std::io::Error>;
            fn from_bytes(bytes: &[u8]) -> Self;
        }

        impl<const N: usize> $name for [u8; N] {
            const SIZE: usize = N;

            #[inline]
            fn write(&self, mut writer: impl Write) -> Result<(), std::io::Error> {
                writer.write_all(self)
            }

            #[inline]
            fn from_bytes(bytes: &[u8]) -> Self {
                <[u8; N]>::try_from(bytes).unwrap()
            }
        }
    };
}

address_trait!(HardwareAddress);

impl HardwareAddress for MacAddress {
    const SIZE: usize = 6;

    #[inline]
    fn write(&self, mut writer: impl Write) -> Result<(), std::io::Error> {
        writer.write_all(&self.0)
    }

    #[inline]
    fn from_bytes(bytes: &[u8]) -> Self {
        <[u8; 6]>::try_from(bytes).unwrap().into()
    }
}

address_trait!(ProtocolAddress);

macro_rules! impl_ip_address {
    ($name:ident, $bytes:expr) => {
        impl ProtocolAddress for $name {
            const SIZE: usize = $bytes;

            #[inline]
            fn write(&self, mut writer: impl Write) -> Result<(), std::io::Error> {
                writer.write_all(&self.octets())
            }

            #[inline]
            fn from_bytes(bytes: &[u8]) -> Self {
                <[u8; $bytes]>::try_from(bytes).unwrap().into()
            }
        }
    };
}

impl_ip_address!(Ipv4Addr, 4);
impl_ip_address!(Ipv6Addr, 16);

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

    #[inline]
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Reply),
            2 => Ok(Self::Reply),
            _ => Err(InvalidOperation { value }),
        }
    }
}

impl From<Operation> for u16 {
    #[inline]
    fn from(value: Operation) -> Self {
        match value {
            Operation::Request => 1,
            Operation::Reply => 2,
        }
    }
}

#[derive(Debug, Default)]
pub struct Service {
    cache: HashMap<IpAddr, CacheEntry>,
}

impl Service {
    pub fn insert(&mut self, ip_address: IpAddr, hardware_address: MacAddress, is_self: bool) {
        self.cache.insert(
            ip_address,
            CacheEntry {
                hardware_address,
                is_self,
            },
        );
    }

    pub async fn handle_request<'a>(
        &mut self,
        request: &'a ArpPacketSlice<'a>,
        mut sender: impl Sender,
    ) -> Result<(), Error> {
        handle_request_for_ipvx::<Ipv4Addr>(&mut self.cache, request, &mut sender).await?;
        handle_request_for_ipvx::<Ipv6Addr>(&mut self.cache, request, &mut sender).await?;

        Ok(())
    }
}

#[derive(Clone, Debug)]
struct CacheEntry {
    hardware_address: MacAddress,
    is_self: bool,
}

pub trait Sender {
    fn send<H: HardwareAddress, P: ProtocolAddress>(
        &mut self,
        response: &ArpPacket<H, P>,
    ) -> impl Future<Output = Result<(), super::packet::SendError>>;
}

async fn handle_request_for_ipvx<'a, A>(
    cache: &mut HashMap<IpAddr, CacheEntry>,
    request: &'a ArpPacketSlice<'a>,
    sender: &mut impl Sender,
) -> Result<(), Error>
where
    A: ProtocolAddress + Copy + Display,
    IpAddr: From<A>,
    ArpPacket<MacAddress, A>: TryFrom<ArpPacketSlice<'a>>,
{
    if let Ok(ArpPacket {
        operation,
        sender_hardware_address,
        sender_protocol_address,
        target_protocol_address,
        ..
    }) = ArpPacket::<MacAddress, A>::try_from(request.clone())
    {
        let mut merge_flag = false;

        if let Some(entry) = cache.get_mut(&sender_protocol_address.into()) {
            tracing::debug!(protocol_address = %sender_protocol_address, hardware_address = %sender_hardware_address, "merge");
            entry.hardware_address = sender_hardware_address;
            merge_flag = true;
        }

        if let Some(CacheEntry {
            hardware_address,
            is_self: true,
        }) = cache.get(&target_protocol_address.into()).cloned()
        {
            if !merge_flag {
                tracing::debug!(protocol_address = %sender_protocol_address, hardware_address = %sender_hardware_address, "new");

                cache.insert(
                    sender_protocol_address.into(),
                    CacheEntry {
                        hardware_address: sender_hardware_address,
                        is_self: false,
                    },
                );
            }
            if operation == Operation::Request {
                // note: the RFC states that sender and target get swapped, but doesn't mention
                // that we have to insert our hardware address???
                let reply = ArpPacket {
                    hardware_type: request.hardware_type,
                    protocol_type: request.protocol_type,
                    operation: Operation::Reply,
                    sender_hardware_address: hardware_address,
                    sender_protocol_address: target_protocol_address,
                    target_hardware_address: sender_hardware_address,
                    target_protocol_address: sender_protocol_address,
                };

                tracing::debug!(protocol_address = %target_protocol_address, %hardware_address, "reply");
                sender
                    .send(&reply)
                    .await
                    .map_err(|e| Error::Send(e.into()))?;
            }
        }
    }

    Ok(())
}
