//! ARP protocol implementation.
//!
//! # References
//! - [An Ethernet Address Resolution Protocol](https://datatracker.ietf.org/doc/html/rfc826)
//! - [Address Resolution Protocol (ARP) Parameters](https://www.iana.org/assignments/arp-parameters/arp-parameters.xhtml)

use std::{
    collections::HashMap,
    fmt::{
        Debug,
        Display,
    },
    io::Write,
    net::{
        IpAddr,
        Ipv4Addr,
        Ipv6Addr,
    },
};

use byst::io::{
    read::{
        read,
        Read,
    },
    Cursor,
};
use futures::Future;

pub type ProtocolType = crate::protocol::inet::ethernet::EtherType;

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

/// Length of ARP packet without addresses.
const FIXED_SIZE: usize = 8;

fn test_read() {
    let mut cursor = Cursor::new(b"");
    let arp = read!(cursor => ArpPacket<MacAddress, Ipv4Addr>).unwrap();

    todo!();
}

#[derive(Clone, Debug, Read)]
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

mod test2 {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Read)]
    #[byst(discriminant(ty = "u16", big))]
    pub enum Operation {
        #[byst(discriminant = 1)]
        Request,
        #[byst(discriminant = 2)]
        Reply,
    }
}

pub use self::test2::Operation;

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

/// Represents an ARP protocol hardware identifier.
///
/// You can access the underlying `u16` value by using `.0` and any `u16`
/// can be converted to an [`HardwareType`]:
#[derive(Clone, Copy, Eq, PartialEq, Hash, Read, derive_more::From, derive_more::Into)]
pub struct HardwareType(#[byst(network)] pub u16);

impl HardwareType {
    // Numbers sourced from https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/plain/include/uapi/linux/if_arp.h?id=e33c4963bf536900f917fb65a687724d5539bc21

    /// from KA9Q: NET/ROM pseudo
    pub const NETROM: Self = Self(0);

    /// Ethernet 10Mbps
    pub const ETHER: Self = Self(1);

    /// Experimental Ethernet
    pub const EETHER: Self = Self(2);

    /// AX.25 Level 2
    pub const AX25: Self = Self(3);

    /// PROnet token ring
    pub const PRONET: Self = Self(4);

    /// Chaosnet
    pub const CHAOS: Self = Self(5);

    /// IEEE 802.2 Ethernet/TR/TB
    pub const IEEE802: Self = Self(6);

    /// ARCnet
    pub const ARCNET: Self = Self(7);

    /// APPLEtalk
    pub const APPLETLK: Self = Self(8);

    /// Frame Relay DLCI
    pub const DLCI: Self = Self(15);

    /// ATM
    pub const ATM: Self = Self(19);

    /// Metricom STRIP (new IANA id)
    pub const METRICOM: Self = Self(23);

    /// IEEE 1394 IPv4 - RFC 2734
    pub const IEEE1394: Self = Self(24);

    /// EUI-64
    pub const EUI64: Self = Self(27);

    /// InfiniBand
    pub const INFINIBAND: Self = Self(32);

    /// SLIP
    pub const SLIP: Self = Self(256);

    /// CSLIP
    pub const CSLIP: Self = Self(257);

    /// SLIP6
    pub const SLIP6: Self = Self(258);

    /// CSLIP6
    pub const CSLIP6: Self = Self(259);

    /// Notional KISS type
    pub const RSRVD: Self = Self(260);

    /// ADAPT
    pub const ADAPT: Self = Self(264);

    /// ROSE
    pub const ROSE: Self = Self(270);

    /// CCITT X.25
    pub const X25: Self = Self(271);

    /// Boards with X.25 in firmware
    pub const HWX25: Self = Self(272);

    /// Controller Area Network
    pub const CAN: Self = Self(280);

    /// PPP
    pub const PPP: Self = Self(512);

    /// Cisco HDLC
    pub const CISCO_HDLC: Self = Self(513);

    /// LAPB
    pub const LAPB: Self = Self(516);

    /// Digital's DDCMP protocol
    pub const DDCMP: Self = Self(517);

    /// Raw HDLC
    pub const RAWHDLC: Self = Self(518);

    /// Raw IP
    pub const RAWIP: Self = Self(519);

    /// IPIP tunnel
    pub const TUNNEL: Self = Self(768);

    /// IP6IP6 tunnel
    pub const TUNNEL6: Self = Self(769);

    /// Frame Relay Access Device
    pub const FRAD: Self = Self(770);

    /// SKIP vif
    pub const SKIP: Self = Self(771);

    /// Loopback device
    pub const LOOPBACK: Self = Self(772);

    /// Localtalk device
    pub const LOCALTLK: Self = Self(773);

    /// Fiber Distributed Data Interface
    pub const FDDI: Self = Self(774);

    /// AP1000 BIF
    pub const BIF: Self = Self(775);

    /// sit0 device - IPv6-in-IPv4
    pub const SIT: Self = Self(776);

    /// IP over DDP tunneller
    pub const IPDDP: Self = Self(777);

    /// GRE over IP
    pub const IPGRE: Self = Self(778);

    /// PIMSM register interface
    pub const PIMREG: Self = Self(779);

    /// High Performance Parallel Interface
    pub const HIPPI: Self = Self(780);

    /// Nexus 64Mbps Ash
    pub const ASH: Self = Self(781);

    /// Acorn Econet
    pub const ECONET: Self = Self(782);

    /// Linux-IrDA
    pub const IRDA: Self = Self(783);

    /// Point to point fibrechannel
    pub const FCPP: Self = Self(784);

    /// Fibrechannel arbitrated loop
    pub const FCAL: Self = Self(785);

    /// Fibrechannel public loop
    pub const FCPL: Self = Self(786);

    /// Fibrechannel fabric
    pub const FCFABRIC: Self = Self(787);

    /// Magic type ident for TR
    pub const IEEE802_TR: Self = Self(800);

    /// IEEE 802.11
    pub const IEEE80211: Self = Self(801);

    /// IEEE 802.11 + Prism2 header
    pub const IEEE80211_PRISM: Self = Self(802);

    /// IEEE 802.11 + radiotap header
    pub const IEEE80211_RADIOTAP: Self = Self(803);

    /// IEEE 802.15.4
    pub const IEEE802154: Self = Self(804);

    /// IEEE 802.15.4 network monitor
    pub const IEEE802154_MONITOR: Self = Self(805);

    /// PhoNet media type
    pub const PHONET: Self = Self(820);

    /// PhoNet pipe header
    pub const PHONET_PIPE: Self = Self(821);

    /// CAIF media type
    pub const CAIF: Self = Self(822);

    /// GRE over IPv6
    pub const IP6GRE: Self = Self(823);

    /// Netlink header
    pub const NETLINK: Self = Self(824);

    /// IPv6 over LoWPAN
    pub const IPV6LOWPAN: Self = Self(825);

    /// Vsock monitor header
    pub const VSOCKMON: Self = Self(826);

    pub const VOID: Self = Self(0xFFFF);
    pub const NONE: Self = Self(0xFFFE);

    pub fn name(&self) -> Option<&'static str> {
        Some(match *self {
            Self::NETROM => "from KA9Q: NET/ROM pseudo",
            Self::ETHER => "Ethernet 10Mbps",
            Self::EETHER => "Experimental Ethernet",
            Self::AX25 => "AX.25 Level 2",
            Self::PRONET => "PROnet token ring",
            Self::CHAOS => "Chaosnet",
            Self::IEEE802 => "IEEE 802.2 Ethernet/TR/TB",
            Self::ARCNET => "ARCnet",
            Self::APPLETLK => "APPLEtalk",
            Self::DLCI => "Frame Relay DLCI",
            Self::ATM => "ATM",
            Self::METRICOM => "Metricom STRIP (new IANA id)",
            Self::IEEE1394 => "IEEE 1394 IPv4 - RFC 2734",
            Self::EUI64 => "EUI-64",
            Self::INFINIBAND => "InfiniBand",
            Self::SLIP => "SLIP",
            Self::CSLIP => "CSLIP",
            Self::SLIP6 => "SLIP6",
            Self::CSLIP6 => "CSLIP6",
            Self::RSRVD => "Notional KISS type",
            Self::ADAPT => "ADAPT",
            Self::ROSE => "ROSE",
            Self::X25 => "CCITT X.25",
            Self::HWX25 => "Boards with X.25 in firmware",
            Self::CAN => "Controller Area Network",
            Self::PPP => "PPP",
            Self::CISCO_HDLC => "Cisco HDLC",
            Self::LAPB => "LAPB",
            Self::DDCMP => "Digital's DDCMP protocol",
            Self::RAWHDLC => "Raw HDLC",
            Self::RAWIP => "Raw IP",
            Self::TUNNEL => "IPIP tunnel",
            Self::TUNNEL6 => "IP6IP6 tunnel",
            Self::FRAD => "Frame Relay Access Device",
            Self::SKIP => "SKIP vif",
            Self::LOOPBACK => "Loopback device",
            Self::LOCALTLK => "Localtalk device",
            Self::FDDI => "Fiber Distributed Data Interface",
            Self::BIF => "AP1000 BIF",
            Self::SIT => "sit0 device - IPv6-in-IPv4",
            Self::IPDDP => "IP over DDP tunneller",
            Self::IPGRE => "GRE over IP",
            Self::PIMREG => "PIMSM register interface",
            Self::HIPPI => "High Performance Parallel Interface",
            Self::ASH => "Nexus 64Mbps Ash",
            Self::ECONET => "Acorn Econet",
            Self::IRDA => "Linux-IrDA",
            Self::FCPP => "Point to point fibrechannel",
            Self::FCAL => "Fibrechannel arbitrated loop",
            Self::FCPL => "Fibrechannel public loop",
            Self::FCFABRIC => "Fibrechannel fabric",
            Self::IEEE802_TR => "Magic type ident for TR",
            Self::IEEE80211 => "IEEE 802.11",
            Self::IEEE80211_PRISM => "IEEE 802.11 + Prism2 header",
            Self::IEEE80211_RADIOTAP => "IEEE 802.11 + radiotap header",
            Self::IEEE802154 => "IEEE 802.15.4",
            Self::IEEE802154_MONITOR => "IEEE 802.15.4 network monitor",
            Self::PHONET => "PhoNet media type",
            Self::PHONET_PIPE => "PhoNet pipe header",
            Self::CAIF => "CAIF media type",
            Self::IP6GRE => "GRE over IPv6",
            Self::NETLINK => "Netlink header",
            Self::IPV6LOWPAN => "IPv6 over LoWPAN",
            Self::VSOCKMON => "Vsock monitor header",
            _ => return None,
        })
    }
}

impl Debug for HardwareType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(name) = self.name() {
            write!(f, "{} ({name})", self.0)
        }
        else {
            write!(f, "{}", self.0)
        }
    }
}
