//! Internet Control Message Protocol
//!
//! - [RFC 792](https://datatracker.ietf.org/doc/html/rfc792)

use std::net::Ipv4Addr;

use byst::{
    endianness::NetworkEndian,
    io::{
        Read,
        Reader,
        ReaderExt,
        Write,
    },
    Buf,
};

use crate::util::network_enum;

#[derive(Clone, Copy, Debug)]
pub struct Header {
    pub r#type: Type,
    pub code: Code,
    pub checksum: u16,
    pub aux: [u8; 4],
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Read, Write)]
pub struct Type(pub u8);

network_enum! {
    for Type: Debug;

    ECHO_REPLY => 0;
    DESTINATION_UNREACHABLE => 3;
    REDIRECT => 5;
    ECHO_REQUEST => 8;
    ROUTER_ADVERTISEMENT => 9;
    ROUTER_SOLICITATION => 10;
    TIME_EXCEEDED => 11;
    PARAMETER_PROBLEM => 12;
    TIMESTAMP => 13;
    TIMESTAMP_REPLY => 14;
    EXTENDED_ECHO_REQUEST => 42;
    EXTENDED_ECHO_REPLY => 43;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Read, Write)]
pub struct Code(pub u8);

pub enum Message<D> {
    EchoReply(Echo<D>),
    DestinationUnreachable {
        code: DestinationUnreachableCode,
        original: D,
    },
    EchoRequest(Echo<D>),
    TimeExceeded {
        code: TimeExceededCode,
        original: D,
    },
    ParameterProblem {
        pointer: u8,
        original: D,
    },
    Redirect {
        code: RedirectCode,
        gateway: Ipv4Addr,
        original: D,
    },
    Unknown {
        header: Header,
        data: D,
    },
}

impl<'h, R: Reader, D> Read<R, &'h Header> for Message<D>
where
    D: Read<R, (), Error = R::Error>,
{
    type Error = R::Error;

    fn read(reader: &mut R, header: &'h Header) -> Result<Self, Self::Error> {
        match header.r#type {
            Type::ECHO_REPLY => Ok(Self::EchoReply(reader.read_with(header)?)),
            Type::DESTINATION_UNREACHABLE => {
                Ok(Self::DestinationUnreachable {
                    code: header.code.into(),
                    original: reader.read()?,
                })
            }
            Type::ECHO_REQUEST => Ok(Self::EchoRequest(reader.read_with(header)?)),
            Type::TIME_EXCEEDED => {
                Ok(Self::TimeExceeded {
                    code: header.code.into(),
                    original: reader.read()?,
                })
            }
            Type::PARAMETER_PROBLEM => {
                Ok(Self::ParameterProblem {
                    pointer: header.aux[0],
                    original: reader.read()?,
                })
            }
            Type::REDIRECT => {
                Ok(Self::Redirect {
                    code: header.code.into(),
                    gateway: Ipv4Addr::from(header.aux),
                    original: reader.read()?,
                })
            }
            _ => {
                Ok(Self::Unknown {
                    header: *header,
                    data: reader.read()?,
                })
            }
        }
    }
}

#[derive(Clone, Debug, Read, Write)]
pub struct Echo<D> {
    #[byst(network)]
    pub id: u16,

    #[byst(network)]
    pub seq: u16,

    pub data: D,
}

impl<'h, R: Reader, D> Read<R, &'h Header> for Echo<D>
where
    D: Read<R, (), Error = R::Error>,
{
    type Error = R::Error;

    fn read(reader: &mut R, header: &'h Header) -> Result<Self, Self::Error> {
        let mut aux = header.aux.reader();
        let id = aux.read_with(NetworkEndian).unwrap();
        let seq = aux.read_with(NetworkEndian).unwrap();
        let data = reader.read()?;
        Ok(Self { id, seq, data })
    }
}

#[derive(Clone, Copy, Read, Write)]
pub struct DestinationUnreachableCode(pub u8);

impl From<Code> for DestinationUnreachableCode {
    #[inline]
    fn from(value: Code) -> Self {
        Self(value.0)
    }
}

network_enum! {
    for DestinationUnreachableCode: Debug;

    /// Destination network unreachable
    DESTINATION_NETWORK_UNREACHABLE => 0;

    /// Destination host unreachable
    DESTINATION_HOST_UNREACHABE => 1;

    /// Destination protocol unreachable
    DESTINATION_PROTOCOL_UNREACHABLE => 2;

    /// Destination port unreachable
    DESTINATION_PORT_UNREACHABLE => 3;

    /// Fragmentation required, and DF flag set
    FRAGMENTATION_REQUIRED => 4;

    /// Source route failed
    SOURCE_ROUTE_FAILED => 5;

    /// Destination network unknown
    DESTINATION_NETWORK_UNKNOWN => 6;

    /// Destination host unknown
    DESTINATION_HOST_UNKNOWN => 7;

    /// Source host isolated
    SOURCE_HOST_ISOLATED => 8;

    /// Network administratively prohibited
    NETWORK_ADMINISTRATIVELY_PROHIBITED => 9;

    /// Host administratively prohibited
    HOST_ADMINISTRATIVELY_PROHIBITED => 10;

    /// Network unreachable for ToS
    NETWORK_UNREACHABLE_FOR_TOS => 11;

    /// Host unreachable for ToS
    HOST_UNREACHABLE_FOR_TOS => 12;

    /// Communication administratively prohibited
    COMMUNICATION_ADMINISTRATIVELY_PROHIBITED => 13;

    /// Host Precedence Violation
    HOST_PRECEDENCE_VIOLATION => 14;

    /// Precedence cutoff in effect
    PRECEDENCE_CUTOFF_IN_EFFECT => 15;
}

#[derive(Clone, Copy, Read, Write)]
pub struct TimeExceededCode(pub u8);

impl From<Code> for TimeExceededCode {
    #[inline]
    fn from(value: Code) -> Self {
        Self(value.0)
    }
}

network_enum! {
    for TimeExceededCode: Debug;

    /// Time to live exceeded
    TTL_EXCEEDED => 0;

    /// Fragment reassemblt time exceeded
    FRAGMENT_REASSEMBLY_TIME_EXCEEDED => 1;
}

#[derive(Clone, Copy, Read, Write)]
pub struct RedirectCode(pub u8);

impl From<Code> for RedirectCode {
    #[inline]
    fn from(value: Code) -> Self {
        Self(value.0)
    }
}

network_enum! {
    for RedirectCode: Debug;

    /// Redirect datagrams for the Network.
    NETWORK => 0;

    /// Redirect datagrams for the Host.
    HOST => 1;

    /// Redirect datagrams for the Type of Service and Network.
    TOS_AND_NETWORK => 2;

    /// Redirect datagrams for the Type of Service and Host.
    TOS_AND_HOST => 3;
}
