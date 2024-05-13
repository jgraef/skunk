use std::{
    convert::Infallible,
    fmt::Display,
    net::IpAddr,
    str::FromStr,
};

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

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct TcpAddress {
    pub host: HostAddress,
    pub port: u16,
}

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
