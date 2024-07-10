//! Handling of TCP connections.

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

/// Trait representing something that can create TCP connections to a
/// destination address.
///
/// This could be the OS ability to create TCP connections, a proxy client, or
/// event a TOR client.
pub trait Connect {
    type Connection: AsyncRead + AsyncWrite + Send + Sync + Unpin;

    fn connect(
        &self,
        address: &TcpAddress,
    ) -> impl Future<Output = Result<Self::Connection, std::io::Error>> + Send;
}

/// Connect by connecting straight to the address using Tokio.
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

/// Trait for anything that can accept TCP connections.
pub trait Listen {
    type Connection: AsyncRead + AsyncWrite + Send + Sync + Unpin;

    fn accept(&self) -> impl Future<Output = Result<Self::Connection, std::io::Error>> + Send;
}

impl Listen for TcpListener {
    type Connection = TcpStream;

    async fn accept(&self) -> Result<Self::Connection, std::io::Error> {
        let (conn, _) = TcpListener::accept(self).await?;
        Ok(conn)
    }
}
