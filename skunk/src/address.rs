//! Network addresses.

use std::{
    borrow::Cow,
    convert::Infallible,
    fmt::Display,
    net::IpAddr,
    str::FromStr,
};

use lazy_static::lazy_static;
use serde::{
    Deserialize,
    Serialize,
};

/// Either an IP address (IPv4 or IPv6), or a DNS name.
#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum HostAddress {
    IpAddress(IpAddr),
    DnsName(String),
}

impl Display for HostAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IpAddress(ip_address) => write!(f, "{ip_address}"),
            Self::DnsName(name) => write!(f, "{name}"),
        }
    }
}

impl FromStr for HostAddress {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // todo: properly validate the dns name
        Ok(match IpAddr::from_str(s) {
            Ok(ip_address) => HostAddress::IpAddress(ip_address),
            Err(_) => HostAddress::DnsName(s.to_owned()),
        })
    }
}

impl Serialize for HostAddress {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::IpAddress(ip_address) => ip_address.serialize(serializer),
            Self::DnsName(name) => name.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for HostAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s: Cow<'de, str> = Deserialize::deserialize(deserializer)?;
        Ok(s.parse().map_err(serde::de::Error::custom)?)
    }
}

/// A [`HostAddress`] and a port, used for TCP.
#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct TcpAddress {
    pub host: HostAddress,
    pub port: u16,
}

/// Failed to parse [`TcpAddress`].
#[derive(Debug, thiserror::Error)]
#[error("invalid tcp address: {0}")]
pub struct TcpAddressParseError(String);

impl FromStr for TcpAddress {
    type Err = TcpAddressParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || TcpAddressParseError(s.to_owned());
        let colon = s.rfind(':').ok_or_else(err)?;
        let host = s[..colon].parse().map_err(|_| err())?;
        let port = s[colon + 1..].parse().map_err(|_| err())?;
        Ok(Self { host, port })
    }
}

impl Display for TcpAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

impl Serialize for TcpAddress {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TcpAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s: Cow<'de, str> = Deserialize::deserialize(deserializer)?;
        Ok(s.parse().map_err(serde::de::Error::custom)?)
    }
}

#[derive(Clone, Debug)]
pub enum Ports {
    Single(Port),
    /// A range of ports. Represented in string form by `min .. max`. We use
    /// dots, since hyphens can appear in service names.
    Range {
        min: Port,
        max: Port,
    },
}

impl From<u16> for Ports {
    fn from(value: u16) -> Self {
        Self::Single(value.into())
    }
}

impl Display for Ports {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Single(port) => write!(f, "{port}"),
            Self::Range { min, max } => write!(f, "{min}..{max}"),
        }
    }
}

impl<'a> TryFrom<Cow<'a, str>> for Ports {
    type Error = ParsePortError;

    fn try_from(s: Cow<'a, str>) -> Result<Self, Self::Error> {
        let err = |_| ParsePortError(s.to_string());
        if let Some((min, max)) = s.split_once("..") {
            Ok(Self::Range {
                min: min.parse().map_err(err)?,
                max: max.parse().map_err(err)?,
            })
        }
        else {
            Ok(Self::Single(s.try_into()?))
        }
    }
}

impl TryFrom<String> for Ports {
    type Error = ParsePortError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Cow::from(value).try_into()
    }
}

impl<'a> TryFrom<&'a str> for Ports {
    type Error = ParsePortError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Cow::from(value).try_into()
    }
}

impl FromStr for Ports {
    type Err = ParsePortError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Cow::from(s).try_into()
    }
}

impl<'de> Deserialize<'de> for Ports {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(PortsVisitor)
    }
}

impl Serialize for Ports {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Single(port) => port.serialize(serializer),
            Self::Range { min, max } => serializer.serialize_str(&format!("{min}..{max}")),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Port {
    /// Service name as defined by [1].
    ///
    /// [1]: https://www.rfc-editor.org/rfc/rfc6335.html#section-5.1
    Name(String),
    Number(u16),
}

impl Display for Port {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Name(name) => write!(f, "{name}"),
            Self::Number(port) => write!(f, "{port}"),
        }
    }
}

impl From<u16> for Port {
    fn from(value: u16) -> Self {
        Self::Number(value)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid port: {0}")]
pub struct ParsePortError(String);

impl<'a> TryFrom<Cow<'a, str>> for Port {
    type Error = ParsePortError;

    fn try_from(value: Cow<'a, str>) -> Result<Self, Self::Error> {
        match u16::from_str(&value) {
            Ok(port) => Ok(Self::Number(port)),
            Err(_) => {
                lazy_static! {
                    // this ignores the rule that 2 hyphens can't be next to each other 🤷
                    static ref SERVICE_NAME_REGEX: regex::Regex = "^[A-Za-z0-9]([A-Za-z0-9-]{1,13}[A-Za-z0-9])?$".parse().unwrap();
                }
                if SERVICE_NAME_REGEX.is_match(&value) {
                    Ok(Self::Name(value.into_owned()))
                }
                else {
                    Err(ParsePortError(value.into_owned()))
                }
            }
        }
    }
}

impl TryFrom<String> for Port {
    type Error = ParsePortError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(Cow::from(value))
    }
}

impl<'a> TryFrom<&'a str> for Port {
    type Error = ParsePortError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Self::try_from(Cow::from(value))
    }
}

impl FromStr for Port {
    type Err = ParsePortError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(Cow::from(s))
    }
}

impl Serialize for Port {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Name(name) => serializer.serialize_str(name),
            Self::Number(number) => serializer.serialize_u16(*number),
        }
    }
}

impl<'de> Deserialize<'de> for Port {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        //let s: Cow<'de, str> = Deserialize::deserialize(deserializer)?;
        //s.try_into().map_err(serde::de::Error::custom)

        // todo: we need to parse either a string or an int
        deserializer.deserialize_any(PortVisitor)
    }
}

struct PortsVisitor;

impl<'de> serde::de::Visitor<'de> for PortsVisitor {
    type Value = Ports;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter
            .write_str("either a numeric port or a service name or a .. separated range of these")
    }

    fn visit_i8<E>(self, value: i8) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(u16::try_from(value)
            .map_err(serde::de::Error::custom)?
            .into())
    }

    fn visit_u8<E>(self, value: u8) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(u16::from(value).into())
    }

    fn visit_i16<E>(self, value: i16) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(u16::try_from(value)
            .map_err(serde::de::Error::custom)?
            .into())
    }

    fn visit_u16<E>(self, value: u16) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(value.into())
    }

    fn visit_i32<E>(self, value: i32) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(u16::try_from(value)
            .map_err(serde::de::Error::custom)?
            .into())
    }

    fn visit_u32<E>(self, value: u32) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(u16::try_from(value)
            .map_err(serde::de::Error::custom)?
            .into())
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(u16::try_from(value)
            .map_err(serde::de::Error::custom)?
            .into())
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(u16::try_from(value)
            .map_err(serde::de::Error::custom)?
            .into())
    }

    fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Ports::try_from(value).map_err(serde::de::Error::custom)?)
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Ports::try_from(value).map_err(serde::de::Error::custom)?)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Ports::try_from(value).map_err(serde::de::Error::custom)?)
    }
}

struct PortVisitor;

impl<'de> serde::de::Visitor<'de> for PortVisitor {
    type Value = Port;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("either a numeric port or a service name")
    }

    fn visit_i8<E>(self, value: i8) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(u16::try_from(value)
            .map_err(serde::de::Error::custom)?
            .into())
    }

    fn visit_u8<E>(self, value: u8) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(u16::from(value).into())
    }

    fn visit_i16<E>(self, value: i16) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(u16::try_from(value)
            .map_err(serde::de::Error::custom)?
            .into())
    }

    fn visit_u16<E>(self, value: u16) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(value.into())
    }

    fn visit_i32<E>(self, value: i32) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(u16::try_from(value)
            .map_err(serde::de::Error::custom)?
            .into())
    }

    fn visit_u32<E>(self, value: u32) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(u16::try_from(value)
            .map_err(serde::de::Error::custom)?
            .into())
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(u16::try_from(value)
            .map_err(serde::de::Error::custom)?
            .into())
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(u16::try_from(value)
            .map_err(serde::de::Error::custom)?
            .into())
    }

    fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Port::try_from(value).map_err(serde::de::Error::custom)?)
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Port::try_from(value).map_err(serde::de::Error::custom)?)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Port::try_from(value).map_err(serde::de::Error::custom)?)
    }
}
