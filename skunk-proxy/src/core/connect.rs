use async_trait::async_trait;
use tokio::{
    io::{
        AsyncRead,
        AsyncWrite,
    },
    net::TcpStream,
};

use super::address::{
    HostAddress,
    TcpAddress,
};

#[async_trait]
pub trait Connect {
    type Connection: AsyncRead + AsyncWrite + Send + Sync + Unpin;

    async fn connect(&self, address: &TcpAddress) -> Result<Self::Connection, std::io::Error>;
}

#[derive(Clone, Copy, Debug)]
pub struct ConnectTcp;

#[async_trait]
impl Connect for ConnectTcp {
    type Connection = TcpStream;

    async fn connect(&self, address: &TcpAddress) -> Result<Self::Connection, std::io::Error> {
        let stream = match &address.host {
            HostAddress::IpAddress(ip) => TcpStream::connect((*ip, address.port)).await?,
            HostAddress::DnsName(dns_name) => {
                TcpStream::connect((dns_name.as_str(), address.port)).await?
            }
        };
        Ok(stream)
    }
}
