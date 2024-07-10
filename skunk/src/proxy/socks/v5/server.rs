//! SOCKS Protocol Version 5
//!
//! [RFC 1928](https://datatracker.ietf.org/doc/html/rfc1928)

use std::{
    net::{
        IpAddr,
        Ipv4Addr,
        Ipv6Addr,
    },
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use futures::Future;
use tokio::io::{
    AsyncRead,
    AsyncReadExt,
    AsyncWrite,
    AsyncWriteExt,
    ReadBuf,
};

use super::{
    AddressType,
    AuthMethod,
    Command,
    RejectReason,
    Reply,
    SelectedAuthMethod,
};
use crate::{
    address::{
        HostAddress,
        TcpAddress,
        UdpAddress,
    },
    proxy::socks::error::Error,
};

pub async fn serve<S, A>(mut socket: S, auth: &A) -> Result<Request<S, A>, Error>
where
    S: AsyncRead + AsyncWrite + Unpin,
    A: AuthProvider,
{
    let version = socket.read_u8().await?;
    if version == 5 {
        serve_without_version(socket, auth).await
    }
    else {
        Err(Error::InvalidVersion(version))
    }
}

async fn serve_without_version<S, A>(mut socket: S, auth: &A) -> Result<Request<S, A>, Error>
where
    S: AsyncRead + AsyncWrite + Unpin,
    A: AuthProvider,
{
    let n_methods = socket.read_u8().await?;

    let mut auth_methods = vec![AuthMethod::NoAuthentication; n_methods.into()];
    for _ in 0..n_methods {
        auth_methods.push(AuthMethod::try_from(socket.read_u8().await?)?);
    }

    let auth_method = auth.select_method(&auth_methods);

    socket.write_u8(5).await?;
    socket.write_u8(auth_method.into()).await?;
    socket.flush().await?;

    let (mut socket, auth_data) = match auth_method {
        SelectedAuthMethod::Selected(auth_method) => {
            match auth.authenticate(auth_method, socket).await? {
                AuthResult::Success { socket, data } => (socket, data),
                AuthResult::Failed { mut socket } => {
                    socket.shutdown().await?;
                    return Err(Error::AuthenticationFailed);
                }
            }
        }
        SelectedAuthMethod::NoAcceptable => return Err(Error::AuthenticationFailed),
    };

    let version = socket.read_u8().await?;
    if version != 5 {
        return Err(Error::InvalidVersion(version));
    }

    let command = Command::try_from(socket.read_u8().await?)?;

    let reserved = socket.read_u8().await?;
    if reserved != 0 {
        return Err(Error::InvalidRequest);
    }

    let address_type = AddressType::try_from(socket.read_u8().await?)?;

    let host_address = match address_type {
        AddressType::IpV4 => {
            let mut buf = [0u8; 4];
            socket.read_exact(&mut buf).await?;
            HostAddress::IpAddress(Ipv4Addr::from(buf).into())
        }
        AddressType::DomainName => {
            let n = socket.read_u8().await?;
            let mut buf = vec![0; n as usize];
            socket.read_exact(&mut buf).await?;
            HostAddress::DnsName(String::from_utf8(buf).map_err(|_| Error::InvalidHostName)?)
        }
        AddressType::IpV6 => {
            let mut buf = [0u8; 16];
            socket.read_exact(&mut buf).await?;
            HostAddress::IpAddress(Ipv6Addr::from(buf).into())
        }
    };

    let port = socket.read_u16().await?;

    let request = match command {
        Command::Connect => {
            Request::Connect(Connect {
                socket,
                auth_data,
                destination_address: TcpAddress::new(host_address, port),
            })
        }
        Command::Bind => {
            Request::Bind(Bind {
                socket,
                auth_data,
                destination_address: TcpAddress::new(host_address, port),
            })
        }
        Command::Associate => {
            Request::Associate(Associate {
                socket,
                auth_data,
                destination_address: UdpAddress::new(host_address, port),
            })
        }
    };

    Ok(request)
}

pub enum Request<S, A>
where
    S: AsyncRead + AsyncWrite + Unpin,
    A: AuthProvider,
{
    Connect(Connect<S, A>),
    Bind(Bind<S, A>),
    Associate(Associate<S, A>),
}

impl<S, A> Request<S, A>
where
    S: AsyncRead + AsyncWrite + Unpin,
    A: AuthProvider,
{
    pub fn auth_data(&self) -> &A::Data {
        match self {
            Request::Connect(connect) => connect.auth_data(),
            Request::Bind(bind) => bind.auth_data(),
            Request::Associate(associate) => associate.auth_data(),
        }
    }
}

pub struct Connect<S, A>
where
    S: AsyncRead + AsyncWrite + Unpin,
    A: AuthProvider,
{
    socket: A::Socket<S>,
    auth_data: A::Data,
    destination_address: TcpAddress,
}

impl<S, A> Connect<S, A>
where
    S: AsyncRead + AsyncWrite + Unpin,
    A: AuthProvider,
{
    pub fn auth_data(&self) -> &A::Data {
        &self.auth_data
    }

    pub fn destination_address(&self) -> &TcpAddress {
        &self.destination_address
    }

    pub async fn accept(mut self, bind_address: &TcpAddress) -> Result<Connected<S, A>, Error> {
        send_reply(
            &mut self.socket,
            Reply::Succeeded,
            Some((&bind_address.host, bind_address.port)),
        )
        .await?;
        Ok(Connected {
            socket: self.socket,
        })
    }

    pub async fn reject(mut self, failure: RejectReason) -> Result<(), Error> {
        send_reply(&mut self.socket, failure.into(), None).await?;
        self.socket.shutdown().await?;
        Ok(())
    }
}

pub struct Bind<S, A>
where
    S: AsyncRead + AsyncWrite + Unpin,
    A: AuthProvider,
{
    socket: A::Socket<S>,
    auth_data: A::Data,
    destination_address: TcpAddress,
}

impl<S, A> Bind<S, A>
where
    S: AsyncRead + AsyncWrite + Unpin,
    A: AuthProvider,
{
    pub fn auth_data(&self) -> &A::Data {
        &self.auth_data
    }

    pub fn destination_address(&self) -> &TcpAddress {
        &self.destination_address
    }

    pub async fn accept(mut self, bind_address: &TcpAddress) -> Result<Accept<S, A>, Error> {
        send_reply(
            &mut self.socket,
            Reply::Succeeded,
            Some((&bind_address.host, bind_address.port)),
        )
        .await?;
        Ok(Accept {
            socket: self.socket,
        })
    }

    pub async fn reject(mut self, failure: RejectReason) -> Result<(), Error> {
        send_reply(&mut self.socket, failure.into(), None).await?;
        self.socket.shutdown().await?;
        Ok(())
    }
}

/// # TODO
///
/// implement this.
#[derive(Debug)]
pub struct Associate<S, A>
where
    S: AsyncRead + AsyncWrite + Unpin,
    A: AuthProvider,
{
    socket: A::Socket<S>,
    auth_data: A::Data,
    destination_address: UdpAddress,
}

impl<S, A> Associate<S, A>
where
    S: AsyncRead + AsyncWrite + Unpin,
    A: AuthProvider,
{
    pub fn auth_data(&self) -> &A::Data {
        &self.auth_data
    }

    pub fn destination_address(&self) -> &UdpAddress {
        &self.destination_address
    }

    pub async fn reject(mut self, failure: RejectReason) -> Result<(), Error> {
        send_reply(&mut self.socket, failure.into(), None).await?;
        self.socket.shutdown().await?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Connected<S, A>
where
    S: AsyncRead + AsyncWrite + Unpin,
    A: AuthProvider,
{
    socket: A::Socket<S>,
}

impl<S, A> AsyncRead for Connected<S, A>
where
    S: AsyncRead + AsyncWrite + Unpin,
    A: AuthProvider,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.socket).poll_read(cx, buf)
    }
}

impl<S, A> AsyncWrite for Connected<S, A>
where
    S: AsyncRead + AsyncWrite + Unpin,
    A: AuthProvider,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.socket).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.socket).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.socket).poll_shutdown(cx)
    }
}

pub struct Accept<S, A>
where
    S: AsyncRead + AsyncWrite + Unpin,
    A: AuthProvider,
{
    socket: A::Socket<S>,
}

impl<S, A> Accept<S, A>
where
    S: AsyncRead + AsyncWrite + Unpin,
    A: AuthProvider,
{
    pub async fn accept(mut self, peer_address: &TcpAddress) -> Result<Connected<S, A>, Error> {
        send_reply(
            &mut self.socket,
            Reply::Succeeded,
            Some((&peer_address.host, peer_address.port)),
        )
        .await?;
        Ok(Connected {
            socket: self.socket,
        })
    }
}

async fn send_reply<S>(
    mut socket: S,
    reply: Reply,
    bind_address: Option<(&HostAddress, u16)>,
) -> Result<(), Error>
where
    S: AsyncWrite + Unpin,
{
    socket.write_u8(5).await?;
    socket.write_u8(reply.into()).await?;
    socket.write_u8(0).await?;

    if let Some((bind_address, bind_port)) = bind_address {
        match bind_address {
            HostAddress::IpAddress(IpAddr::V4(ip_address)) => {
                socket.write_u8(AddressType::IpV4.into()).await?;
                socket.write_all(&ip_address.octets()).await?;
            }
            HostAddress::IpAddress(IpAddr::V6(ip_address)) => {
                socket.write_u8(AddressType::IpV6.into()).await?;
                socket.write_all(&ip_address.octets()).await?;
            }
            HostAddress::DnsName(name) => {
                socket.write_u8(AddressType::DomainName.into()).await?;
                socket
                    .write_u8(name.len().try_into().map_err(|_| Error::InvalidHostName)?)
                    .await?;
                socket.write_all(name.as_bytes()).await?;
            }
        }

        socket.write_u16(bind_port).await?;
    }
    else {
        // todo: what is expected when the command fails?
        socket.write_u8(AddressType::IpV4.into()).await?;
        socket.write_u32(0).await?;
        socket.write_u16(0).await?;
    }

    socket.flush().await?;

    Ok(())
}

pub enum AuthResult<S, A>
where
    S: AsyncRead + AsyncWrite + Unpin,
    A: AuthProvider + ?Sized,
{
    Success { socket: A::Socket<S>, data: A::Data },
    Failed { socket: S },
}

pub trait AuthProvider {
    type Data;
    type Socket<S>: AsyncRead + AsyncWrite + Unpin
    where
        S: AsyncRead + AsyncWrite + Unpin;

    fn select_method(&self, methods: &[AuthMethod]) -> SelectedAuthMethod;

    fn authenticate<S>(
        &self,
        method: AuthMethod,
        socket: S,
    ) -> impl Future<Output = Result<AuthResult<S, Self>, Error>>
    where
        S: AsyncRead + AsyncWrite + Unpin;
}
