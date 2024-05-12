use std::net::SocketAddr;

use futures_util::Future;
use tokio::{
    io::{
        AsyncRead,
        AsyncWrite,
    },
    net::TcpStream,
};

use super::{
    address::{
        HostAddress,
        TcpAddress,
    },
    filter::Extract,
};

pub trait Connect {
    type Connection: AsyncRead + AsyncWrite + Send + Sync + Unpin;

    fn connect(
        &self,
        address: &TcpAddress,
    ) -> impl Future<Output = Result<Self::Connection, std::io::Error>> + Send;
}

#[derive(Clone, Copy, Debug)]
pub struct ConnectTcp;

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

pub struct PeerAddress(pub SocketAddr);

impl<'a> Extract<'a, PeerAddress> for TcpStream {
    type Error = std::io::Error;

    fn extract(&'a self) -> Result<PeerAddress, Self::Error> {
        Ok(PeerAddress(self.peer_addr()?))
    }
}

pub struct LocalAddress(pub SocketAddr);

impl<'a> Extract<'a, LocalAddress> for TcpStream {
    type Error = std::io::Error;

    fn extract(&'a self) -> Result<LocalAddress, Self::Error> {
        Ok(LocalAddress(self.local_addr()?))
    }
}
