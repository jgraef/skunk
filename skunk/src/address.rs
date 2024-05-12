use std::{
    fmt::Display,
    net::IpAddr,
};

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub struct TcpAddress {
    pub host: HostAddress,
    pub port: u16,
}
