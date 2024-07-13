//! Internet Protocol Version 6
//!
//! - [RFC 8200](https://datatracker.ietf.org/doc/html/rfc8200)
//! - [IPv6 Parameters](https://www.iana.org/assignments/ipv6-parameters/ipv6-parameters.xhtml)

use std::{
    convert::Infallible,
    net::Ipv6Addr,
};

use byst::{
    endianness::NetworkEndian,
    io::{
        Limit,
        Read,
        Reader,
        ReaderExt,
        Write,
    },
    Bytes,
};
use smallvec::SmallVec;

use super::ipv4::{
    self,
    Protocol,
};
use crate::util::network_enum;

#[derive(Clone, Debug)]
pub struct Packet<P = Bytes> {
    pub header: Header,
    pub extension_headers: ExtensionHeaders,

    /// Payload.
    ///
    /// May be `None` if the last `next_header` was set to
    /// [`ExtensionHeaderType::NO_NEXT_HEADER`].
    // todo: we could also use an enum { Payload<P>, None(Bytes) } here
    pub payload: Option<P>,
}

impl<R: Reader, P, E> Read<R, ()> for Packet<P>
where
    P: for<'r> Read<Limit<&'r mut R>, Protocol, Error = E>,
{
    type Error = InvalidPacket<R::Error, E>;

    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        let header: Header = reader.read()?;
        let extension_headers: ExtensionHeaders = reader.read_with(header.next_header)?;

        let mut limit = reader.limit(header.payload_length.into());

        let payload = if let Some(protocol) = extension_headers.protocol() {
            Some(limit.read_with(protocol).map_err(InvalidPacket::Payload)?)
        }
        else {
            None
        };

        let _ = limit.skip_remaining();

        Ok(Self {
            header,
            extension_headers,
            payload,
        })
    }
}

/// [IPv6 header](https://datatracker.ietf.org/doc/html/rfc8200#section-3)
///
/// ```plain
///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///  |Version| Traffic Class |           Flow Label                  |
///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///  |         Payload Length        |  Next Header  |   Hop Limit   |
///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///  |                                                               |
///  +                                                               +
///  |                                                               |
///  +                         Source Address                        +
///  |                                                               |
///  +                                                               +
///  |                                                               |
///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///  |                                                               |
///  +                                                               +
///  |                                                               |
///  +                      Destination Address                      +
///  |                                                               |
///  +                                                               +
///  |                                                               |
///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Header {
    /// 4-bit Internet Protocol version number = 6.
    pub version: u8,

    /// 8-bit Traffic Class field.
    ///
    /// See [Section 7](https://datatracker.ietf.org/doc/html/rfc8200#section-7).
    pub traffic_class: TrafficClass,

    /// 20-bit flow label.
    ///
    /// See [Section 6][1], [RFC][2]
    ///
    /// [1]: https://datatracker.ietf.org/doc/html/rfc8200#section-6
    /// [2]: https://datatracker.ietf.org/doc/html/rfc6437
    pub flow_label: FlowLabel,

    /// Length of the IPv6 payload
    pub payload_length: u16,

    /// Identifies the type of header immediately following the IPv6 header.
    ///
    /// This is either a [`ExtensionHeaderType`] or
    /// [`ProtocolType`][super::ipv4::ProtocolType].
    pub next_header: NextHeader,

    /// 8-bit unsigned integer. Decremented by 1 by each node that forwards the
    /// packet. When forwarding, the packet is discarded if Hop Limit was zero
    /// when received or is decremented to zero. A node that is the destination
    /// of a packet should not discard a packet with Hop Limit equal to zero; it
    /// should process the packet normally.
    pub hop_limit: u8,

    /// 128-bit address of the originator of the packet.
    pub source_address: Ipv6Addr,

    /// 128-bit address of the intended recipient of the packet.
    ///
    /// Possibly not the ultimate recipient, if a Routing header is present.
    pub destination_address: Ipv6Addr,
}

impl<R: Reader> Read<R, ()> for Header {
    type Error = InvalidHeader<R::Error>;

    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        let value: u32 = reader.read_with(NetworkEndian)?;

        let version = (value & 0xf) as u8;
        if version != 6 {
            return Err(InvalidHeader::InvalidVersion { value: version });
        }

        let traffic_class = TrafficClass::from((value >> 4) as u8);
        let flow_label = FlowLabel(value >> 12);

        let payload_length = reader.read_with(NetworkEndian)?;
        let next_header = reader.read()?;
        let hop_limit = reader.read()?;
        let source_address = reader.read()?;
        let destination_address = reader.read()?;

        Ok(Self {
            version,
            traffic_class,
            flow_label,
            payload_length,
            next_header,
            hop_limit,
            source_address,
            destination_address,
        })
    }
}

// todo: also use this for ipv4
#[derive(Clone, Copy, Debug)]
pub struct TrafficClass {
    ds: u8,
    ecn: u8,
}

impl From<u8> for TrafficClass {
    fn from(value: u8) -> Self {
        Self {
            ds: (value >> 2),
            ecn: (value & 3),
        }
    }
}

/// 20-bit flow label.
///
/// See [Section 6][1], [RFC][2]
///
/// [1]: https://datatracker.ietf.org/doc/html/rfc8200#section-6
/// [2]: https://datatracker.ietf.org/doc/html/rfc6437
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Read, Write)]
pub struct FlowLabel(#[byst(network)] u32);

impl FlowLabel {
    pub const UNLABELLED: Self = Self(0);
}

impl FlowLabel {
    pub fn new(value: u32) -> Option<Self> {
        if value & 0xfff00000 == 0 {
            Some(Self(value))
        }
        else {
            None
        }
    }

    pub fn new_unchecked(value: u32) -> Self {
        Self(value)
    }

    pub fn generate_random() -> Self {
        // > It is therefore RECOMMENDED
        // > that source hosts support the flow label by setting the flow label
        // > field for all packets of a given flow to the same value chosen from
        // > an approximation to a discrete uniform distribution.
        todo!();
    }

    pub fn generate_for(
        _source_address: Ipv6Addr,
        _source_port: u16,
        _destination_address: Ipv6Addr,
        _destination_port: u16,
        _protocol: Protocol,
    ) -> Self {
        // https://datatracker.ietf.org/doc/html/rfc6437#appendix-A
        // alternative just use Hash::hash
        todo!();
    }
}

impl From<FlowLabel> for u32 {
    #[inline]
    fn from(value: FlowLabel) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Read, Write)]
pub struct NextHeader(pub u8);

impl From<NextHeader> for Protocol {
    #[inline]
    fn from(value: NextHeader) -> Self {
        Self(value.0)
    }
}

impl From<NextHeader> for ExtensionHeaderType {
    #[inline]
    fn from(value: NextHeader) -> Self {
        Self(value.0)
    }
}

impl From<ipv4::Protocol> for NextHeader {
    #[inline]
    fn from(value: ipv4::Protocol) -> Self {
        Self(value.0)
    }
}

impl From<ExtensionHeaderType> for NextHeader {
    #[inline]
    fn from(value: ExtensionHeaderType) -> Self {
        Self(value.0)
    }
}

#[derive(Clone, Debug)]
pub struct ExtensionHeaders {
    /// The actual extension headers.
    ///
    /// This only heap-allocates if there are more than 9 headers, which should
    /// suffice for most packets.
    ///
    /// Although we want to lookup headers by their type, this vec is very
    /// small, so a linear map is fine.
    extension_headers: SmallVec<[ExtensionHeader; 9]>,
    protocol: Option<Protocol>,
}

impl ExtensionHeaders {
    pub fn get(&self, ty: ExtensionHeaderType) -> Option<&ExtensionHeader> {
        self.extension_headers
            .iter()
            .find(|header| header.header_type() == ty)
    }

    /// Returns the `next_header` value to be put into the IPv6 header.
    pub fn next_header(&self) -> NextHeader {
        self.extension_headers
            .first()
            .map(|header| header.header_type().into())
            .or_else(|| self.protocol.map(Into::into))
            .unwrap_or(ExtensionHeaderType::NO_NEXT_HEADER.into())
    }

    /// Returns the payload's protocol, if there is any (i.e.
    /// [`ExtensionHeaderType::NO_NEXT_HEADER`] isn't present).
    #[inline]
    pub fn protocol(&self) -> Option<Protocol> {
        self.protocol
    }
}

impl<R: Reader> Read<R, NextHeader> for ExtensionHeaders {
    type Error = InvalidExtensionHeader<R::Error>;

    fn read(reader: &mut R, mut next_header: NextHeader) -> Result<Self, Self::Error> {
        let mut is_first = false;
        let mut num_destination_options = 0;
        let mut previous_was_destination_options = false;
        let mut previous_was_fragment = false;
        let mut extension_headers = SmallVec::new();
        let mut protocol = None;

        loop {
            let extension_headers_type = ExtensionHeaderType::from(next_header);
            if extension_headers_type == ExtensionHeaderType::NO_NEXT_HEADER {
                // no next extension header or payload
                // todo: ideally we would tell the Packet Read impl to not parse the payload
                break;
            }
            else if extension_headers_type.is_known_value() {
                // extension header
                let extension_header: ExtensionHeader = reader.read_with(extension_headers_type)?;

                // check if hop-by-hop options are first
                match &extension_header {
                    ExtensionHeader::HopByHopOptions(_) if !is_first => {
                        return Err(InvalidExtensionHeader::HopByHopNotFirst);
                    }
                    _ => {}
                }
                is_first = false;

                // if previous header was destination option, check that this is a routing
                // header
                match &extension_header {
                    ExtensionHeader::Routing(_) => {}
                    _ if previous_was_destination_options => {
                        return Err(InvalidExtensionHeader::DestinationOptionsInInvalidPosition);
                    }
                    _ => {}
                }

                // fragment must be last
                if previous_was_fragment {
                    return Err(InvalidExtensionHeader::FragmentNotLast);
                }

                // set destination option related state and check that there are at most 2
                // destination option headers.
                match &extension_header {
                    ExtensionHeader::DestinationOptions(_) => {
                        previous_was_destination_options = true;
                        num_destination_options += 1;
                        if num_destination_options > 2 {
                            return Err(InvalidExtensionHeader::MoreThan2DestinationOptions);
                        }
                    }
                    _ => {
                        previous_was_destination_options = false;
                    }
                }

                match &extension_header {
                    ExtensionHeader::Fragment(_) => {
                        previous_was_fragment = true;
                    }
                    _ => {
                        previous_was_fragment = false;
                    }
                }

                next_header = extension_header.next_header();
                extension_headers.push(extension_header);
            }
            else {
                // end of extension headers
                protocol = Some(next_header.into());
            }
        }

        Ok(Self {
            extension_headers,
            protocol,
        })
    }
}

/// [IPv6 Extension Headers][1]
///
/// [1]: https://datatracker.ietf.org/doc/html/rfc8200#section-4
#[derive(Clone, Debug)]
pub enum ExtensionHeader {
    HopByHopOptions(extension_headers::HopByHop),
    Routing(extension_headers::Routing),
    Fragment(extension_headers::Fragment),
    AuthenticationHeader(extension_headers::AuthenticationHeader),
    EncapsulationSecurityPayload(extension_headers::EncapsulationSecurityPayload),
    DestinationOptions(extension_headers::DestinationOptions),
    Mobility(extension_headers::Mobility),
    HostIdentityProtocol(extension_headers::HostIdentityProtocol),
    Shim6Protocol(extension_headers::Shim6Protocol),
}

impl ExtensionHeader {
    pub fn next_header(&self) -> NextHeader {
        match self {
            ExtensionHeader::HopByHopOptions(inner) => inner.next_header,
            ExtensionHeader::Routing(inner) => inner.next_header,
            ExtensionHeader::Fragment(inner) => inner.next_header,
            ExtensionHeader::AuthenticationHeader(inner) => inner.next_header,
            ExtensionHeader::EncapsulationSecurityPayload(inner) => inner.next_header,
            ExtensionHeader::DestinationOptions(inner) => inner.next_header,
            ExtensionHeader::Mobility(inner) => inner.next_header,
            ExtensionHeader::HostIdentityProtocol(inner) => inner.next_header,
            ExtensionHeader::Shim6Protocol(inner) => inner.next_header,
        }
    }

    pub fn header_type(&self) -> ExtensionHeaderType {
        match self {
            ExtensionHeader::HopByHopOptions(_) => ExtensionHeaderType::HOP_BY_HOP_OPTIONS,
            ExtensionHeader::Routing(_) => ExtensionHeaderType::ROUTING,
            ExtensionHeader::Fragment(_) => ExtensionHeaderType::FRAGMENT,
            ExtensionHeader::AuthenticationHeader(_) => ExtensionHeaderType::AUTHENTICATION_HEADER,
            ExtensionHeader::EncapsulationSecurityPayload(_) => {
                ExtensionHeaderType::ENCAPSULATION_SECURITY_PAYLOAD
            }
            ExtensionHeader::DestinationOptions(_) => ExtensionHeaderType::DESTINATION_OPTIONS,
            ExtensionHeader::Mobility(_) => ExtensionHeaderType::MOBILITY,
            ExtensionHeader::HostIdentityProtocol(_) => ExtensionHeaderType::HOST_IDENTITY_PROTOCOL,
            ExtensionHeader::Shim6Protocol(_) => ExtensionHeaderType::SHIM6_PROTOCOL,
        }
    }
}

impl<R: Reader> Read<R, ExtensionHeaderType> for ExtensionHeader {
    type Error = InvalidExtensionHeader<R::Error>;

    fn read(reader: &mut R, header_type: ExtensionHeaderType) -> Result<Self, Self::Error> {
        match header_type {
            ExtensionHeaderType::HOP_BY_HOP_OPTIONS => Ok(Self::HopByHopOptions(reader.read()?)),
            ExtensionHeaderType::ROUTING => Ok(Self::Routing(reader.read()?)),
            ExtensionHeaderType::FRAGMENT => Ok(Self::Fragment(reader.read()?)),
            ExtensionHeaderType::DESTINATION_OPTIONS => {
                Ok(Self::DestinationOptions(reader.read()?))
            }
            _ => Err(InvalidExtensionHeader::Unsupported(header_type)),
        }
    }
}

/// This overlaps with ipv4::Protocol (when in Header or DestinationOptions)
#[derive(Clone, Copy, PartialEq, Eq, Read, Write)]
pub struct ExtensionHeaderType(pub u8);

network_enum! {
    for ExtensionHeaderType: Debug;

    HOP_BY_HOP_OPTIONS => 0;
    ROUTING => 43;
    FRAGMENT => 44;
    AUTHENTICATION_HEADER => 51;
    ENCAPSULATION_SECURITY_PAYLOAD => 50;
    DESTINATION_OPTIONS => 60;
    MOBILITY => 135;
    HOST_IDENTITY_PROTOCOL => 139;
    SHIM6_PROTOCOL => 140;
    NO_NEXT_HEADER => 59;
}

pub mod extension_headers {
    use byst::{
        endianness::NetworkEndian,
        io::{
            Read,
            Reader,
            ReaderExt,
            Write,
        },
    };
    use smallvec::SmallVec;

    use super::{
        InvalidExtensionHeader,
        NextHeader,
    };
    use crate::util::network_enum;

    #[derive(Clone, Debug)]
    pub struct HopByHop {
        pub next_header: NextHeader,
        pub options: Options,
    }

    impl<R: Reader> Read<R, ()> for HopByHop {
        type Error = InvalidExtensionHeader<R::Error>;

        fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
            let (next_header, options) = read_options(reader)?;
            Ok(Self {
                next_header,
                options,
            })
        }
    }

    #[derive(Clone, Debug)]
    pub struct DestinationOptions {
        pub next_header: NextHeader,
        pub options: Options,
    }

    impl<R: Reader> Read<R, ()> for DestinationOptions {
        type Error = InvalidExtensionHeader<R::Error>;

        fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
            let (next_header, options) = read_options(reader)?;
            Ok(Self {
                next_header,
                options,
            })
        }
    }

    /// Routing extension header
    ///
    /// [Routing types](https://www.iana.org/assignments/ipv6-parameters/ipv6-parameters.xhtml#ipv6-parameters-3)
    #[derive(Clone, Debug)]
    pub struct Routing {
        pub next_header: NextHeader,
        pub segments_left: u8,
        pub routing_data: RoutingData,
    }

    impl<R: Reader> Read<R, ()> for Routing {
        type Error = InvalidExtensionHeader<R::Error>;

        fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
            let next_header: NextHeader = reader.read()?;

            // length of extension in multiples of 8 octets, not including the first 8
            // octets
            let header_extension_length: u8 = reader.read()?;

            let routing_type: RoutingType = reader.read()?;
            let segments_left: u8 = reader.read()?;

            let data_length = usize::from(header_extension_length) * 8 + 4;
            let mut limit = reader.limit(data_length);

            let routing_data = if segments_left == 0 {
                RoutingData::Unrecognized { routing_type }
            }
            else {
                return Err(InvalidExtensionHeader::DiscardPacket);
            };

            limit.skip_remaining()?;

            Ok(Self {
                next_header,
                segments_left,
                routing_data,
            })
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq, Hash, Read, Write)]
    pub struct RoutingType(pub u8);

    network_enum! {
        for RoutingType: Debug;
    }

    #[derive(Clone, Debug)]
    pub enum RoutingData {
        Unrecognized { routing_type: RoutingType },
    }

    #[derive(Clone, Debug)]
    pub struct Fragment {
        pub next_header: NextHeader,
        pub fragment_offset: u16,
        pub more: bool,
        pub identification: u16,
    }

    impl<R: Reader> Read<R, ()> for Fragment {
        type Error = InvalidExtensionHeader<R::Error>;

        fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
            let next_header: NextHeader = reader.read()?;
            reader.skip(1)?;

            let value: u16 = reader.read_with(NetworkEndian)?;
            let fragment_offset = value & 0x1fff;
            let more = value & 0x8000 != 0;

            let identification = reader.read_with(NetworkEndian)?;

            Ok(Self {
                next_header,
                fragment_offset,
                more,
                identification,
            })
        }
    }

    #[derive(Clone, Debug)]
    pub struct AuthenticationHeader {
        pub next_header: NextHeader,
        // todo
    }

    #[derive(Clone, Debug)]
    pub struct EncapsulationSecurityPayload {
        pub next_header: NextHeader,
        // todo
    }

    #[derive(Clone, Debug)]
    pub struct Mobility {
        pub next_header: NextHeader,
        // todo
    }

    #[derive(Clone, Debug)]
    pub struct HostIdentityProtocol {
        pub next_header: NextHeader,
        // todo
    }

    #[derive(Clone, Debug)]
    pub struct Shim6Protocol {
        pub next_header: NextHeader,
        // todo
    }

    fn read_options<R: Reader>(
        reader: &mut R,
    ) -> Result<(NextHeader, Options), InvalidExtensionHeader<R::Error>> {
        let next_header: NextHeader = reader.read()?;

        // length of extension in multiples of 8 octets, not including the first 8
        // octets
        let header_extension_length: u8 = reader.read()?;

        let options_length = usize::from(header_extension_length) * 8 + 6;

        let mut limit = reader.limit(options_length);
        let options = limit.read()?;
        limit.skip_remaining()?;

        Ok((next_header, options))
    }

    /// Hop-by-Hop and destination options
    ///
    /// [RFC 8200 Section 4.2](https://datatracker.ietf.org/doc/html/rfc8200#section-4.2)
    /// [Option types](https://www.iana.org/assignments/ipv6-parameters/ipv6-parameters.xhtml#ipv6-parameters-2)
    #[derive(Clone, Debug)]
    pub struct Options {
        options: SmallVec<[Option; 4]>,
    }

    impl<R: Reader> Read<R, ()> for Options {
        type Error = InvalidOption<R::Error>;

        fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
            let options = SmallVec::new();

            loop {
                let Ok(ty) = reader.read::<OptionType>()
                else {
                    break;
                };

                match ty {
                    OptionType::PAD1 => {
                        // nop
                    }
                    OptionType::PADN => {
                        let length: u8 = reader.read()?;
                        reader.skip(length.into())?;
                    }
                    _ => {
                        tracing::debug!("unrecognized option: {:#02x}", ty.0);

                        match ty.unrecognized_action() {
                            UnrecognizedAction::Skip => {
                                let length: u8 = reader.read()?;
                                reader.skip(length.into())?;
                            }
                            UnrecognizedAction::Discard { icmp_response } => {
                                return Err(InvalidOption::DiscardUnrecognizedOption {
                                    icmp_response,
                                })
                            }
                        }
                    }
                }
            }

            Ok(Self { options })
        }
    }

    #[derive(Clone, Debug)]
    pub struct Option {
        pub ty: OptionType,
        pub length: u8,
        pub data: OptionData,
    }

    #[derive(Clone, Copy, PartialEq, Eq, Hash, Read, Write)]
    pub struct OptionType(pub u8);

    network_enum! {
        for OptionType: Debug;

        PAD1 => 0;
        PADN => 1;
    }

    impl OptionType {
        pub fn unrecognized_action(&self) -> UnrecognizedAction {
            match self.0 & 0xc0 {
                0x00 => UnrecognizedAction::Skip,
                0x40 => {
                    UnrecognizedAction::Discard {
                        icmp_response: DiscardIcmpResponse::Never,
                    }
                }
                0x80 => {
                    UnrecognizedAction::Discard {
                        icmp_response: DiscardIcmpResponse::Always,
                    }
                }
                0xc0 => {
                    UnrecognizedAction::Discard {
                        icmp_response: DiscardIcmpResponse::IfNotMulticast,
                    }
                }
                _ => unreachable!(),
            }
        }

        pub fn can_change_enroute(&self) -> bool {
            self.0 & 0x20 != 0
        }
    }

    #[derive(Clone, Copy, Debug)]
    pub enum UnrecognizedAction {
        Skip,
        Discard { icmp_response: DiscardIcmpResponse },
    }

    #[derive(Clone, Copy, Debug)]
    pub enum DiscardIcmpResponse {
        Never,
        Always,
        IfNotMulticast,
    }

    #[derive(Clone, Debug)]
    pub enum OptionData {}

    #[derive(Debug, thiserror::Error)]
    #[error("Invalid extension header option")]
    pub enum InvalidOption<R> {
        Read(#[from] R),

        #[error("Options contained an unrecognized option for which we should discard the packet")]
        DiscardUnrecognizedOption {
            icmp_response: DiscardIcmpResponse,
            // todo: we need to include the IP header and 64bit of the payload for the ICMP
            // response
        },
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid IPv6 packet")]
pub enum InvalidPacket<R, P = Infallible> {
    Header(#[from] InvalidHeader<R>),
    ExtensionHeader(#[from] InvalidExtensionHeader<R>),
    Payload(#[source] P),
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid IPv6 header")]
pub enum InvalidHeader<R> {
    Read(#[from] R),

    #[error("Invalid IP version: {value}")]
    InvalidVersion {
        value: u8,
    },

    #[error("Invalid internet header length: {value}")]
    InvalidInternetHeaderLength {
        value: u8,
    },
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid IPv6 extension header")]
pub enum InvalidExtensionHeader<R> {
    Read(#[from] R),

    #[error("Hop-by-hop extension header is not the first extension header")]
    HopByHopNotFirst,

    #[error("Destination options extension header in invalid position")]
    DestinationOptionsInInvalidPosition,

    #[error("Fragment header must be last")]
    FragmentNotLast,

    #[error("More than 2 destination options extension headers")]
    MoreThan2DestinationOptions,

    #[error("Unknown extension header: {0:?}")]
    Unsupported(ExtensionHeaderType),

    InvalidOptions(#[from] extension_headers::InvalidOption<R>),

    // todo: merge this with InvalidOption::DiscardUnrecognizedOption
    // todo: add stuff to reply with the correct ICMP packet. we need to know the position from
    // the start of the IPv6 packet for this!
    #[error("Discarding packet")]
    DiscardPacket,
}
