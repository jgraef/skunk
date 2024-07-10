pub mod client;
pub mod server;

use super::error::{
    InvalidCommand,
    InvalidReply,
};

/// The default port to use for the server.
pub const DEFAULT_PORT: u16 = 9090;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthMethod {
    NoAuthentication,
    GssApi,
    UsernamePassword,
    Iana(u8),
    Private(u8),
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid authentication method: {value:02x}")]
pub struct InvalidAuthMethod {
    pub value: u8,
}

impl TryFrom<u8> for AuthMethod {
    type Error = InvalidAuthMethod;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(Self::NoAuthentication),
            0x01 => Ok(Self::GssApi),
            0x02 => Ok(Self::UsernamePassword),
            _ if value >= 0x03 && value <= 0x7f => Ok(Self::Iana(value)),
            _ if value >= 0x80 && value <= 0xfe => Ok(Self::Private(value)),
            0xff => Err(InvalidAuthMethod { value }),
            _ => unreachable!(),
        }
    }
}

impl From<AuthMethod> for u8 {
    fn from(value: AuthMethod) -> Self {
        match value {
            AuthMethod::NoAuthentication => 0x00,
            AuthMethod::GssApi => 0x01,
            AuthMethod::UsernamePassword => 0x02,
            AuthMethod::Iana(value) => value,
            AuthMethod::Private(value) => value,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectedAuthMethod {
    Selected(AuthMethod),
    NoAcceptable,
}

impl From<u8> for SelectedAuthMethod {
    fn from(value: u8) -> Self {
        AuthMethod::try_from(value).map_or(Self::NoAcceptable, Self::Selected)
    }
}

impl From<SelectedAuthMethod> for u8 {
    fn from(value: SelectedAuthMethod) -> Self {
        match value {
            SelectedAuthMethod::Selected(auth_method) => auth_method.into(),
            SelectedAuthMethod::NoAcceptable => 0xff,
        }
    }
}

impl From<AuthMethod> for SelectedAuthMethod {
    fn from(value: AuthMethod) -> Self {
        Self::Selected(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Connect,
    Bind,
    Associate,
}

impl TryFrom<u8> for Command {
    type Error = InvalidCommand;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(Self::Connect),
            0x02 => Ok(Self::Bind),
            0x03 => Ok(Self::Associate),
            _ => Err(InvalidCommand { value }),
        }
    }
}

impl From<Command> for u8 {
    fn from(value: Command) -> Self {
        match value {
            Command::Connect => 0x01,
            Command::Bind => 0x02,
            Command::Associate => 0x03,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressType {
    IpV4,
    DomainName,
    IpV6,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid address type: 0x{value:02x}")]
pub struct InvalidAddressType {
    pub value: u8,
}

impl TryFrom<u8> for AddressType {
    type Error = InvalidAddressType;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(Self::IpV4),
            0x03 => Ok(Self::DomainName),
            0x04 => Ok(Self::IpV6),
            _ => Err(InvalidAddressType { value }),
        }
    }
}

impl From<AddressType> for u8 {
    fn from(value: AddressType) -> Self {
        match value {
            AddressType::IpV4 => 0x01,
            AddressType::DomainName => 0x03,
            AddressType::IpV6 => 0x04,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reply {
    Succeeded,
    Failure(RejectReason),
}

impl TryFrom<u8> for Reply {
    type Error = InvalidReply;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(Self::Succeeded),
            _ => Ok(Self::Failure(value.try_into()?)),
        }
    }
}

impl From<Reply> for u8 {
    fn from(value: Reply) -> Self {
        match value {
            Reply::Succeeded => 0x00,
            Reply::Failure(value) => value.into(),
        }
    }
}

impl Reply {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failure(value) => value.message(),
        }
    }
}

impl From<RejectReason> for Reply {
    fn from(value: RejectReason) -> Self {
        Self::Failure(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RejectReason {
    GeneralFailure,
    NotAllowed,
    NetworkUnreachable,
    HostUnreachable,
    ConnectionRefused,
    TTLExpired,
    CommandNotSupported,
    AddressTypeNotSupported,
}

impl TryFrom<u8> for RejectReason {
    type Error = InvalidReply;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(Self::GeneralFailure),
            0x02 => Ok(Self::NotAllowed),
            0x03 => Ok(Self::NetworkUnreachable),
            0x04 => Ok(Self::HostUnreachable),
            0x05 => Ok(Self::ConnectionRefused),
            0x06 => Ok(Self::TTLExpired),
            0x07 => Ok(Self::CommandNotSupported),
            0x08 => Ok(Self::AddressTypeNotSupported),
            _ => Err(InvalidReply { value }),
        }
    }
}

impl From<RejectReason> for u8 {
    fn from(value: RejectReason) -> Self {
        match value {
            RejectReason::GeneralFailure => 0x01,
            RejectReason::NotAllowed => 0x02,
            RejectReason::NetworkUnreachable => 0x03,
            RejectReason::HostUnreachable => 0x04,
            RejectReason::ConnectionRefused => 0x05,
            RejectReason::TTLExpired => 0x06,
            RejectReason::CommandNotSupported => 0x07,
            RejectReason::AddressTypeNotSupported => 0x08,
        }
    }
}

impl RejectReason {
    pub fn message(&self) -> &'static str {
        match self {
            Self::GeneralFailure => "general SOCKS server failure",
            Self::NotAllowed => "connection not allowed by ruleset",
            Self::NetworkUnreachable => "Network unreachable",
            Self::HostUnreachable => "Host unreachable",
            Self::ConnectionRefused => "Connection refused",
            Self::TTLExpired => "TTL expired",
            Self::CommandNotSupported => "Command not supported",
            Self::AddressTypeNotSupported => "Address type not supported",
        }
    }
}
