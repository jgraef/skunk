//! Linux SLL Packet

use byst::{
    endianness::NetworkEndian,
    io::{
        read,
        Read,
    },
};

use super::{
    arp::HardwareType,
    ethernet::EtherType,
};
use crate::util::network_enum;

/// A Linux SLL header
///
/// The header format is described here([1]).
///
/// [1]: https://www.tcpdump.org/linktypes/LINKTYPE_LINUX_SLL.html
pub struct SllHeader {
    pub packet_type: PacketType,
    pub hardware_type: HardwareType,
    pub link_layer_address_length: u16,
    pub link_layer_address: [u8; 8],
    pub protocol_type: ProtocolType,
}

/// SLL packet types
///
/// Sourced from [here][1].
///
/// > `PACKET_FASTROUTE` and `PACKET_LOOPBACK` are invisible to user space.
///
/// [1]: https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/plain/include/uapi/linux/if_packet.h?id=e33c4963bf536900f917fb65a687724d5539bc21
#[derive(Clone, Copy, Debug, Read, PartialEq, Eq)]
pub struct PacketType(#[byst(network)] u16);

network_enum! {
    for PacketType

    /// To us
    HOST => 0;

    /// To all
    BROADCAST => 1;

    /// To group
    MULTICAST => 2;

    /// To someone else
    OTHERHOST => 3;

    /// Outgoing of any type
    OUTGOING => 4;

    /// MC/BRD frame looped back
    LOOPBACK => 5;

    /// To user space
    USER => 6;

    /// To kernel space
    KERNEL => 7;
}

/// SLL packet protocol type.
///
/// This can be an [`EtherType`], or a few other variants.
///
/// See [1]
///
/// [1]: https://www.tcpdump.org/linktypes/LINKTYPE_LINUX_SLL.html
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolType {
    Netlink(u16),
    GenericRoutingEncapsulation(u16),
    EtherType(EtherType),
    LinuxNonstandardEtherType(LinuxNonstandardEtherType),
    Unknown(u16),
}

impl From<(HardwareType, u16)> for ProtocolType {
    fn from((hardware_type, value): (HardwareType, u16)) -> Self {
        match hardware_type {
            HardwareType::NETLINK => Self::Netlink(value),
            HardwareType::IPGRE => Self::GenericRoutingEncapsulation(value),
            HardwareType::ETHER => {
                match LinuxNonstandardEtherType::try_from(value) {
                    Ok(v) => Self::LinuxNonstandardEtherType(v),
                    Err(_) => Self::EtherType(EtherType(value)),
                }
            }
            _ => Self::Unknown(value),
        }
    }
}

impl<R> Read<R, HardwareType> for ProtocolType
where
    u16: Read<R, NetworkEndian>,
{
    type Error = <u16 as Read<R, NetworkEndian>>::Error;

    fn read(reader: &mut R, hardware_type: HardwareType) -> Result<Self, Self::Error> {
        let value = read!(reader => u16; NetworkEndian)?;
        Ok(Self::from((hardware_type, value)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Read)]
pub struct LinuxNonstandardEtherType(#[byst(network)] u16);

network_enum! {
    for LinuxNonstandardEtherType

    N802_3 => 0x0001;
    AX25 => 0x0002;
    ALL => 0x0003;
    N802_2 => 0x0004;
    SNAP => 0x0005;
    DDCMP => 0x0006;
    WAN_PPP => 0x0007;
    PPP_MP => 0x0008;
    LOCALTALK => 0x0009;
    CAN => 0x000C;
    CANFD => 0x000D;
    CANXL => 0x000E;
    PPPTALK => 0x0010;
    TR_802_2 => 0x0011;
    MOBITEX => 0x0015;
    CONTROL => 0x0016;
    IRDA => 0x0017;
    ECONET => 0x0018;
    HDLC => 0x0019;
    ARCNET => 0x001A;
    DSA => 0x001B;
    TRAILER => 0x001C;
    PHONET => 0x00F5;
    IEEE802154 => 0x00F6;
    CAIF => 0x00F7;
    XDSA => 0x00F8;
    MAP => 0x00F9;
    MCTP => 0x00FA;
}

impl TryFrom<u16> for LinuxNonstandardEtherType {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        // this is a little bit of a dirty trick to know if we have this value defined
        // as constant.
        let ty = LinuxNonstandardEtherType(value);
        ty.name().map(|_| ty).ok_or(())
    }
}
