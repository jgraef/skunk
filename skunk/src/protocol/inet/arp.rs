//! ARP protocol implementation.
//!
//! # References
//! - [An Ethernet Address Resolution Protocol](https://datatracker.ietf.org/doc/html/rfc826)
//! - [Address Resolution Protocol (ARP) Parameters](https://www.iana.org/assignments/arp-parameters/arp-parameters.xhtml)

use std::{
    fmt::Debug,
    net::IpAddr,
};

use byst::io::{
    read,
    BufReader,
    End,
    Read,
};

use super::MacAddress;
use crate::util::{
    network_enum,
    punctuated,
};

/// An ARP packet.
#[derive(Clone, Debug)]
pub struct Packet {
    pub hardware_type: HardwareType,
    pub protocol_type: ProtocolType,
    pub hardware_address_length: u8,
    pub protocol_address_length: u8,
    pub operation: Operation,
    pub sender_hardware_address: MacAddress,
    pub sender_protocol_address: IpAddr,
    pub target_hardware_address: MacAddress,
    pub target_protocol_address: IpAddr,
}

impl<R> Read<R, ()> for Packet
where
    R: BufReader,
{
    type Error = InvalidPacket;

    fn read(reader: &mut R, _params: ()) -> Result<Self, Self::Error> {
        let hardware_type = read!(reader => HardwareType)?;
        let protocol_type = read!(reader => ProtocolType)?;
        let hardware_address_length = read!(reader => u8)?;
        let protocol_address_length = read!(reader => u8)?;
        let operation = read!(reader => Operation)?;

        match hardware_type {
            HardwareType::ETHER => {
                if hardware_address_length != 6 {
                    return Err(InvalidPacket::InvalidHardwareAddressLength {
                        expected: 6,
                        got: hardware_address_length,
                    });
                }
            }
            _ => {
                return Err(InvalidPacket::InvalidHardwareType {
                    expected: &[HardwareType::ETHER],
                    got: hardware_type,
                })
            }
        }

        match protocol_type {
            ProtocolType::IPV4 => {
                if protocol_address_length != 4 {
                    return Err(InvalidPacket::InvalidProtocolAddressLength {
                        expected: 4,
                        got: protocol_address_length,
                    });
                }
            }
            ProtocolType::IPV6 => {
                if protocol_address_length != 16 {
                    return Err(InvalidPacket::InvalidProtocolAddressLength {
                        expected: 16,
                        got: protocol_address_length,
                    });
                }
            }
            _ => {
                return Err(InvalidPacket::InvalidProtocolType {
                    expected: &[ProtocolType::IPV4, ProtocolType::IPV6],
                    got: protocol_type,
                })
            }
        }

        let sender_hardware_address = read!(reader => MacAddress)?;

        let sender_protocol_address = match protocol_type {
            ProtocolType::IPV4 => IpAddr::V4(read!(reader)?),
            ProtocolType::IPV6 => IpAddr::V6(read!(reader)?),
            _ => unreachable!(),
        };

        let target_hardware_address = read!(reader => MacAddress)?;

        let target_protocol_address = match protocol_type {
            ProtocolType::IPV4 => IpAddr::V4(read!(reader)?),
            ProtocolType::IPV6 => IpAddr::V6(read!(reader)?),
            _ => unreachable!(),
        };

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
}

// todo: remove alias
type ProtocolType = super::ethernet::EtherType;

#[derive(Debug, thiserror::Error)]
#[error("Invalid ARP packet")]
pub enum InvalidPacket {
    #[error(
        "Invalid hardware type: expected {:?}, but got {got:?}",
        punctuated(expected, ", ")
    )]
    InvalidHardwareType {
        expected: &'static [HardwareType],
        got: HardwareType,
    },

    #[error(
        "Invalid protocol type: expected {:?}, but got {got:?}",
        punctuated(expected, ", ")
    )]
    InvalidProtocolType {
        expected: &'static [ProtocolType],
        got: ProtocolType,
    },

    #[error("Invalid hardware address length: expected {expected:?}, but got {got:?}")]
    InvalidHardwareAddressLength { expected: usize, got: u8 },

    #[error("Invalid protocol address length: expected {expected:?}, but got {got:?}")]
    InvalidProtocolAddressLength { expected: usize, got: u8 },

    #[error("Invalid operation: {got}")]
    InvalidOperation { got: u32 },

    #[error("Incomplete packet")]
    Incomplete(#[from] End),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Read)]
pub struct Operation(#[byst(network)] pub u16);

network_enum! {
    for Operation

    /// ARP request
    REQUEST => 1;

    /// ARP reply
    REPLY => 2;
}

/// Represents an ARP protocol hardware identifier.
///
/// Numbers sourced from[1]
///
/// [1]: https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/plain/include/uapi/linux/if_arp.h?id=e33c4963bf536900f917fb65a687724d5539bc21
#[derive(Clone, Copy, Eq, PartialEq, Hash, Read)]
pub struct HardwareType(#[byst(network)] pub u16);

network_enum! {
    for HardwareType

    /// from KA9Q: NET/ROM pseudo
    NETROM => 0;

    /// Ethernet 10Mbps
    ETHER => 1;

    /// Experimental Ethernet
    EETHER => 2;

    /// AX.25 Level 2
    AX25 => 3;

    /// PROnet token ring
    PRONET => 4;

    /// Chaosnet
    CHAOS => 5;

    /// IEEE 802.2 Ethernet/TR/TB
    IEEE802 => 6;

    /// ARCnet
    ARCNET => 7;

    /// APPLEtalk
    APPLETLK => 8;

    /// Frame Relay DLCI
    DLCI => 15;

    /// ATM
    ATM => 19;

    /// Metricom STRIP (new IANA id)
    METRICOM => 23;

    /// IEEE 1394 IPv4 - RFC 2734
    IEEE1394 => 24;

    /// EUI-64
    EUI64 => 27;

    /// InfiniBand
    INFINIBAND => 32;

    /// SLIP
    SLIP => 256;

    /// CSLIP
    CSLIP => 257;

    /// SLIP6
    SLIP6 => 258;

    /// CSLIP6
    CSLIP6 => 259;

    /// Notional KISS type
    RSRVD => 260;

    /// ADAPT
    ADAPT => 264;

    /// ROSE
    ROSE => 270;

    /// CCITT X.25
    X25 => 271;

    /// Boards with X.25 in firmware
    HWX25 => 272;

    /// Controller Area Network
    CAN => 280;

    /// PPP
    PPP => 512;

    /// Cisco HDLC
    CISCO_HDLC => 513;

    /// LAPB
    LAPB => 516;

    /// Digital's DDCMP protocol
    DDCMP => 517;

    /// Raw HDLC
    RAWHDLC => 518;

    /// Raw IP
    RAWIP => 519;

    /// IPIP tunnel
    TUNNEL => 768;

    /// IP6IP6 tunnel
    TUNNEL6 => 769;

    /// Frame Relay Access Device
    FRAD => 770;

    /// SKIP vif
    SKIP => 771;

    /// Loopback device
    LOOPBACK => 772;

    /// Localtalk device
    LOCALTLK => 773;

    /// Fiber Distributed Data Interface
    FDDI => 774;

    /// AP1000 BIF
    BIF => 775;

    /// sit0 device - IPv6-in-IPv4
    SIT => 776;

    /// IP over DDP tunneller
    IPDDP => 777;

    /// GRE over IP
    IPGRE => 778;

    /// PIMSM register interface
    PIMREG => 779;

    /// High Performance Parallel Interface
    HIPPI => 780;

    /// Nexus 64Mbps Ash
    ASH => 781;

    /// Acorn Econet
    ECONET => 782;

    /// Linux-IrDA
    IRDA => 783;

    /// Point to point fibrechannel
    FCPP => 784;

    /// Fibrechannel arbitrated loop
    FCAL => 785;

    /// Fibrechannel public loop
    FCPL => 786;

    /// Fibrechannel fabric
    FCFABRIC => 787;

    /// Magic type ident for TR
    IEEE802_TR => 800;

    /// IEEE 802.11
    IEEE80211 => 801;

    /// IEEE 802.11 + Prism2 header
    IEEE80211_PRISM => 802;

    /// IEEE 802.11 + radiotap header
    IEEE80211_RADIOTAP => 803;

    /// IEEE 802.15.4
    IEEE802154 => 804;

    /// IEEE 802.15.4 network monitor
    IEEE802154_MONITOR => 805;

    /// PhoNet media type
    PHONET => 820;

    /// PhoNet pipe header
    PHONET_PIPE => 821;

    /// CAIF media type
    CAIF => 822;

    /// GRE over IPv6
    IP6GRE => 823;

    /// Netlink header
    NETLINK => 824;

    /// IPv6 over LoWPAN
    IPV6LOWPAN => 825;

    /// Vsock monitor header
    VSOCKMON => 826;

    VOID => 0xFFFF;
    NONE => 0xFFFE;

}

impl Debug for HardwareType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(name) = self.name() {
            write!(f, "HardwareType::{name}({:04x})", self.0)
        }
        else {
            write!(f, "HardwareType({:04x})", self.0)
        }
    }
}
