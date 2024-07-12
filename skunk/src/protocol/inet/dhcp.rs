use std::{
    fmt::Debug,
    net::Ipv4Addr,
};

use bitflags::bitflags;
use byst::{
    endianness::NetworkEndian,
    io::{
        Limit,
        Read,
        Reader,
        ReaderExt,
        Write,
    },
    util::for_tuple,
};
use skunk_util::ordered_multimap::OrderedMultiMap;

use crate::util::network_enum;

/// A [DHCP message][1]
///
/// [1]: https://datatracker.ietf.org/doc/html/rfc2131#section-2
#[derive(Clone, Debug, Read, Write)]
pub struct Message {
    pub op: Opcode,
    pub htype: HardwareType,
    pub hlen: u8,
    pub hops: u8,
    #[byst(network)]
    pub xid: u32,
    #[byst(network)]
    pub secs: u16,
    #[byst(network)]
    pub flags: u16,
    pub ciaddr: Ipv4Addr,
    pub yiaddr: Ipv4Addr,
    pub siaddr: Ipv4Addr,
    pub giaddr: Ipv4Addr,
    pub chaddr: [u8; 16],
    pub sname: [u8; 64],
    pub file: [u8; 128],
    pub options: (),
}

#[derive(Clone, Copy, Debug, Read, Write)]
pub struct Opcode(pub u8);

network_enum! {
    for Opcode;

    BOOTREQUEST => 1;
    BOOTREPLY => 2;
}

#[derive(Clone, Copy, Eq, PartialEq, Hash, Read, Write)]
pub struct HardwareType(pub u8);

// todo: I just copied these from arp::HardwareType. On a quick glance they
// seemed to match, but do they really?
network_enum! {
    for HardwareType;

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

bitflags! {
    #[derive(Clone, Copy, Debug)]
    pub struct Flags: u16 {
        const BROADCAST = 0b1000000000000000;
    }
}

impl<R: Reader> Read<R, ()> for Flags {
    type Error = R::Error;

    fn read(reader: &mut R, _params: ()) -> Result<Self, Self::Error> {
        Ok(Self::from_bits_retain(reader.read_with(NetworkEndian)?))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid DHCP message")]
pub enum InvalidMessage<R> {
    Read(#[from] R),

    #[error("Invalid options magic: {value:?}")]
    InvalidOptionsMagic {
        value: [u8; 4],
    },

    #[error("Invalid option: {code:?}")]
    InvalidOption {
        code: OptionCode,
        length: u8,
    },
}

/// BOOTP Vendor extensions
///
/// - [Format][1]
/// - [DHCP options][2]
///
/// [1]: https://datatracker.ietf.org/doc/html/rfc1497
/// [2]: https://datatracker.ietf.org/doc/html/rfc1533
#[derive(Clone, Debug)]
pub struct Options {
    inner: OrderedMultiMap<OptionCode, Option>,
}

impl Options {
    pub const MAGIC: [u8; 4] = [99, 130, 83, 99];
}

impl<R: Reader> Read<R, ()> for Options {
    type Error = InvalidMessage<R::Error>;

    fn read(reader: &mut R, _params: ()) -> Result<Self, Self::Error> {
        let magic = reader.read_byte_array::<4>()?;
        if magic != Self::MAGIC {
            return Err(InvalidMessage::InvalidOptionsMagic { value: magic });
        }

        let mut inner = OrderedMultiMap::new();
        while let Ok(option) = reader.read::<Option>() {
            let code = option.code();
            inner.insert(code, option);
            if code == OptionCode::END {
                break;
            }
        }
        Ok(Self { inner })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Read, Write)]
pub struct OptionCode(pub u8);

macro_rules! make_options {
    {
        $(
            $(#[doc = $doc:expr])?
            $code_const:ident = $code:expr => $name:ident $( ( $( $field:ty ),* ) )?,
        )*
    } => {
        network_enum! {
            for OptionCode;
            $(
                $(#[doc = $doc])?
                $code_const => $code;
            )*
        }

        #[derive(Clone, Debug)]
        pub enum Option {
            $(
                $name(options::$name),
            )*
            Unknown {
                code: OptionCode,
                length: u8,
            }
        }

        impl Option {
            fn code(&self) -> OptionCode {
                match self {
                    $(
                        Self::$name(_) => OptionCode::$code_const,
                    )*
                    Self::Unknown { code, .. } => *code,
                }
            }
        }

        impl<R: Reader> Read<R, ()> for Option
        {
            type Error = InvalidMessage<R::Error>;

            fn read(reader: &mut R, _params: ()) -> Result<Self, Self::Error> {
                let code = reader.read::<OptionCode>()?;

                let option = match code {
                    OptionCode::PAD => Option::Pad(options::Pad),
                    OptionCode::END => Option::End(options::End),
                    _ => {
                        let length = reader.read::<u8>()?;
                        let mut limit = Limit::new(reader, length.into());
                        let option = match code {
                            $(
                                OptionCode::$code_const => {
                                    limit.read().map_err(|_| InvalidMessage::InvalidOption { code, length })?
                                },
                            )*
                            _ => {
                                Option::Unknown {
                                    code,
                                    length,
                                }
                            },
                        };
                        limit.skip_remaining()?;
                        option
                    },
                };

                Ok(option)
            }
        }

        pub mod options {
            use std::net::Ipv4Addr;
            use byst::io::{Reader, Read, ReaderExt};
            use super::{OptionCode, Option, OptionWrapper};

            $(
                $(#[doc = $doc])?
                #[derive(Clone, Debug)]
                pub struct $name $( ( $(pub $field ),* ) )?;

                impl $name {
                    pub const CODE: OptionCode = OptionCode::$code_const;
                }

                impl From<$name> for Option {
                    fn from(value: $name) -> Self {
                        Self::$name(value)
                    }
                }

                impl TryFrom<Option> for $name {
                    type Error = Option;

                    fn try_from(value: Option) -> Result<$name, Option> {
                        match value {
                            Option::$name(option) => Ok(option),
                            _ => Err(value),
                        }
                    }
                }

                impl<R: Reader> Read<R, ()> for $name {
                    type Error = R::Error;

                    #[allow(unused_variables)]
                    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
                        Ok(Self $(
                            (
                                $(
                                    reader.read::<OptionWrapper<$field>>()?.0,
                                )*
                            )
                        )?)
                    }
                }
            )*
        }
    };
}

make_options! {
    PAD = 0 => Pad,
    SUBNET_MASK = 1 => SubnetMask(Ipv4Addr),
    TIME_OFFSET = 2 => TimeOffset(i32),
    END = 255 => End,
    GATEWAY = 3 => Gateway(Vec<Ipv4Addr>),
    TIME_SERVER = 4 => TimeServer(Vec<Ipv4Addr>),
    NAME_SERVER = 5 => NameServer(Vec<Ipv4Addr>),
    DOMAIN_NAME_SERVER = 6 => DomainNameServer(Vec<Ipv4Addr>),
    LOG_SERVER = 7 => LogServer(Vec<Ipv4Addr>),
    QUOTE_SERVER = 8 => QuoteServer(Vec<Ipv4Addr>),
    LPR_SERVER = 9 => LprServer(Vec<Ipv4Addr>),
    IMPRESS_SERVER = 10 => ImpressServer(Vec<Ipv4Addr>),
    RLP_SERVER = 11 => RlpServer(Vec<Ipv4Addr>),
    HOSTNAME = 12 => Hostname(String),
    BOOT_FILE_SIZE = 13 => BootFileSize(u16),
    MERIT_DUMP_FILE = 14 => MeritDumpFile(String),
    DOMAIN_NAME = 15 => DomainName(String),
    SWAP_SERVER = 16 => SwapServer(Ipv4Addr),
    ROOT_PATH = 17 => RootPath(String),
    EXTENSION_PATH = 18 => ExtensionPath(String),
    IP_FORWARDING = 19 => IpForwarding(bool),
    NON_LOCAL_SOURCE_ROUTING = 20 => NonLocalSourceRouting(bool),
    POLICY_FILTER = 21 => PolicyFilter(Vec<(Ipv4Addr, Ipv4Addr)>),
    MAX_DATAGRAM_REASSEMBLY_SIZE = 22 => MaxDatagramReassemblySize(u16),
    DEFAULT_IP_TTL = 23 => DefaultIpTtl(u8),
    PATH_MTU_AGING_TIMEOUT = 24 => PathMtuAgingTimeout(u32),
    PATH_MTU_PLATEAU_TABLE = 25 => PathMtuPlateauTable(Vec<u16>),
    INTERFACE_MTU = 26 => InterfaceMtu(u16),
    ALL_SUBNETS_ARE_LOCAL = 27 => AllSubnetsAreLocal(bool),
    BROADCAST_ADDRESS = 28 => BroadcastAddress(Ipv4Addr),
    PERFORM_MASK_DISCOVERY = 29 => PerformMaskDiscovery(bool),
    MASK_SUPPLIER = 30 => MaskSupplier(bool),
    PERFORM_ROUTER_DISCOVERY = 31 => PerformRouterDiscovery(bool),
    ROUTER_SOLICITATION_ADDRESS = 32 => RouterSolicitationAddress(Ipv4Addr),
    STATIC_ROUTE = 33 => StaticRoute(Vec<(Ipv4Addr, Ipv4Addr)>),
    TRAILER_ENCAPSULATION = 34 => TrailerEncapsulation(bool),
    ARP_CACHE_TIMEOUT = 35 => ArpCacheTimeout(u32),
    ETHERNET_ENCAPSULATION = 36 => EthernetEncapsulation(bool),
    TCP_DEFAULT_TTL = 37 => TcpDefaultTtl(u8),
    TCP_KEEPALIVE_INTERVAL = 38 => TcpKeepaliveInterval(u32),
    TCP_KEEPALIVE_GARBAGE = 39 => TcpKeepaliveGarbage(bool),
    NETWORK_INFORMATION_SERVICE_DOMAIN = 40 => NetworkInformationServiceDomain(String),
    NETWORK_INFORMATION_SERVERS = 41 => NetworkInformationServers(Vec<Ipv4Addr>),
    NETWORK_TIME_PROTOCOL_SERVERS = 42 => NetworkTimeProtocolServers(Vec<Ipv4Addr>),
    VENDOR_SPECIFIC_INFORMATION = 43 => VendorSpecificInformation(), // todo
    NETBIOS_OVER_TCP_IP_NAME_SERVER = 44 => NetbiosOverTcpIpNameServer(Vec<Ipv4Addr>),
    NETBIOS_OVER_TCP_IP_DATAGRAM_DISTRIBUTION_SERVER = 45 => NetbiosOverTcpIpDatagramDistributionServer(Vec<Ipv4Addr>),
    NETBIOS_OVER_TCP_IP_NODE_TYPE = 46 => NetbiosOverTcpIpNodeType(u8),
    NETBIOS_OVER_TCP_IP_SCOPE = 47 => NetbiosOverTcpIpScope(String),
    X_WINDOW_SYSTEM_FONT_SERVER = 48 => XWindowSystemFontServer(Vec<Ipv4Addr>),
    X_WINDOW_SYSTEM_DISPLAY_MANAGER = 49 => XWindowSystemDisplayManager(Vec<Ipv4Addr>),
    REQUESTED_IP_ADDRESS = 50 => RequestedIpAddress(Ipv4Addr),
    IP_ADDRESS_LEASE_TIME = 51 => IpAddressLeaseTime(u32),
    OPTION_OVERLOAD = 52 => OptionOverload(u8),
    DHCP_MESSAGE_TYPE = 53 => DhcpMessageType(u8),
    SERVER_IDENTIFIER = 54 => ServerIdentifier(Ipv4Addr),
    PARAMETER_REQUEST_LIST = 55 => ParameterRequestList(Vec<OptionCode>),
    MESSAGE = 56 => Message(String),
    MAX_DHCP_MESSAGE_SIZE = 57 => MaxDhcpMessageSize(u16),
    RENEWAL_T1_TIME_VALUE = 58 => RenwealT1TimeValue(u32),
    RENEWAL_T2_TIME_VALUE = 59 => RenwealT2TimeValue(u32),
    CLASS_IDENTIFIER = 60 => ClassIdentifier(Vec<u8>),
    CLIENT_IDENTIFIER = 61 => ClientIdentifier(u8, Vec<u8>),
}

network_enum! {
    for options::OptionOverload;

    FILE => 1;
    SNAME => 2;
    BOTH => 3;
}

network_enum! {
    for options::DhcpMessageType;

    DHCPDISCOVER => 1;
    DHCPOFFER => 2;
    DHCPREQUEST => 3;
    DHCPDECLINE => 4;
    DHCPACK => 5;
    DHCPNAK => 6;
    DHCPRELEASE => 7;
}

struct OptionWrapper<T>(T);

impl<R: Reader> Read<R, ()> for OptionWrapper<Ipv4Addr> {
    type Error = <Ipv4Addr as Read<R, ()>>::Error;

    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        Ok(Self(reader.read()?))
    }
}

impl<R: Reader> Read<R, ()> for OptionWrapper<i32> {
    type Error = <i32 as Read<R, NetworkEndian>>::Error;

    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        Ok(Self(reader.read_with(NetworkEndian)?))
    }
}

impl<R: Reader> Read<R, ()> for OptionWrapper<u16> {
    type Error = <u16 as Read<R, NetworkEndian>>::Error;

    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        Ok(Self(reader.read_with(NetworkEndian)?))
    }
}

impl<R: Reader> Read<R, ()> for OptionWrapper<u32> {
    type Error = <u32 as Read<R, NetworkEndian>>::Error;

    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        Ok(Self(reader.read_with(NetworkEndian)?))
    }
}

impl<R: Reader> Read<R, ()> for OptionWrapper<u8> {
    type Error = <u8 as Read<R, ()>>::Error;

    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        Ok(Self(reader.read()?))
    }
}

impl<R: Reader> Read<R, ()> for OptionWrapper<bool> {
    type Error = R::Error;

    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        Ok(Self(reader.read::<u8>()? != 0))
    }
}

impl<R: Reader> Read<R, ()> for OptionWrapper<OptionCode> {
    type Error = R::Error;

    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        Ok(Self(reader.read()?))
    }
}

impl<R: Reader, T> Read<R, ()> for OptionWrapper<Vec<T>>
where
    OptionWrapper<T>: Read<R, ()>,
{
    type Error = R::Error;

    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        // todo: ask reader how many bytes are remaining and allocate vec for it.
        let mut buf = vec![];
        while let Ok(item) = reader.read::<OptionWrapper<T>>() {
            buf.push(item.0);
        }
        Ok(Self(buf))
    }
}

impl<R: Reader> Read<R, ()> for OptionWrapper<String> {
    type Error = R::Error;

    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        // todo: ask reader how many bytes are remaining and allocate String for it.
        let mut buf = vec![];
        reader.read_into(&mut buf, None)?;
        // todo: handle error
        Ok(Self(String::from_utf8(buf).expect("Invalid UTF-8 string")))
    }
}

macro_rules! impl_option_wrapper_for_tuple {
    (
        $index:tt => $name:ident: $ty:ident
    ) => {
        impl_option_wrapper_for_tuple! {
            $index => $name: $ty,
        }
    };
    (
        $first_index:tt => $first_name:ident: $first_ty:ident,
        $($tail_index:tt => $tail_name:ident: $tail_ty:ident),*
    ) => {
        impl<R, $first_ty, $($tail_ty),*> Read<R, ()> for OptionWrapper<($first_ty, $($tail_ty,)*)>
        where
            OptionWrapper<$first_ty>: Read<R, ()>,
            OptionWrapper<$($tail_ty>: Read<R, (), Error = <OptionWrapper<$first_ty> as Read<R, ()>>::Error>,)*
        {
            type Error = <OptionWrapper<$first_ty> as Read<R, ()>>::Error;

            fn read(mut reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
                let $first_name = <OptionWrapper<$first_ty> as Read<R, ()>>::read(&mut reader, ())?;
                $(
                    let $tail_name = <OptionWrapper<$tail_ty> as Read<R, ()>>::read(&mut reader, ())?;
                )*
                Ok(OptionWrapper((
                    $first_name.0,
                    $($tail_name.0,)*
                )))
            }
        }
    };
}
for_tuple!(impl_option_wrapper_for_tuple! for 2..=2);
