use std::future::Future;

use tokio::{
    io::{
        AsyncRead,
        AsyncWrite,
    },
    net::{
        TcpListener,
        TcpStream,
    },
};

use super::address::{
    HostAddress,
    TcpAddress,
};

pub trait Connect {
    type Connection: AsyncRead + AsyncWrite + Send + Sync + Unpin;

    fn connect(
        &self,
        address: &TcpAddress,
    ) -> impl Future<Output = Result<Self::Connection, std::io::Error>> + Send;
}

#[derive(Clone, Copy, Debug, Default)]
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

pub trait Listen {
    type Connection: AsyncRead + AsyncWrite + Send + Sync + Unpin;

    fn accept(&self) -> impl Future<Output = Result<Self::Connection, std::io::Error>> + Send;
}

pub struct ListenTcp {
    listener: TcpListener,
}

impl Listen for ListenTcp {
    type Connection = TcpStream;

    async fn accept(&self) -> Result<Self::Connection, std::io::Error> {
        let (conn, _) = self.listener.accept().await?;
        Ok(conn)
    }
}
