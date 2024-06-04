use std::{
    net::{
        IpAddr,
        Ipv4Addr,
    },
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use bytes::Bytes;
use tokio::io::{
    AsyncRead,
    AsyncReadExt,
    AsyncWrite,
    AsyncWriteExt,
    ReadBuf,
};

use super::{
    Command,
    Reply,
};
use crate::{
    address::{
        HostAddress,
        TcpAddress,
    },
    proxy::socks::error::Error,
    util::io::read_nul_terminated,
};

pub async fn serve<S>(mut socket: S) -> Result<Request<S>, Error>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let version = socket.read_u8().await?;
    if version == 4 {
        serve_without_version(socket).await
    }
    else {
        Err(Error::InvalidVersion(version))
    }
}

async fn serve_without_version<S>(mut socket: S) -> Result<Request<S>, Error>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let command = Command::try_from(socket.read_u8().await?)?;

    let dest_port = socket.read_u16().await?;

    let mut dest_ip = [0u8; 4];
    socket.read_exact(&mut dest_ip).await?;

    let user_id = read_nul_terminated(&mut socket).await?;

    let dest_address = if dest_ip[0] == 0 && dest_ip[1] == 0 && dest_ip[2] == 0 && dest_ip[3] != 0 {
        let dns_name = read_nul_terminated(&mut socket).await?;
        HostAddress::DnsName(
            String::from_utf8(dns_name.into()).map_err(|_| Error::InvalidHostName)?,
        )
    }
    else {
        HostAddress::IpAddress(IpAddr::from(dest_ip))
    };

    let dest_address = TcpAddress::new(dest_address, dest_port);

    let inner = RequestInner {
        socket,
        dest_address,
        user_id: user_id.freeze(),
    };

    Ok(match command {
        Command::Connect => Request::Connect(Connect { inner }),
        Command::Bind => Request::Bind(Bind { inner }),
    })
}

pub enum Request<S> {
    Connect(Connect<S>),
    Bind(Bind<S>),
}

pub struct Connect<S> {
    inner: RequestInner<S>,
}

impl<S> Connect<S> {
    pub fn destination_address(&self) -> &TcpAddress {
        &self.inner.dest_address
    }

    pub fn user_id(&self) -> Bytes {
        self.inner.user_id.clone()
    }
}

impl<S> Connect<S>
where
    S: AsyncWrite + Unpin,
{
    pub async fn accept(mut self, bind_address: (Ipv4Addr, u16)) -> Result<Connected<S>, Error> {
        self.inner
            .send_reply(Reply::Granted, Some(bind_address))
            .await?;
        Ok(Connected {
            socket: self.inner.socket,
        })
    }

    pub async fn reject(mut self) -> Result<(), Error> {
        self.inner.send_reply(Reply::Failed, None).await?;
        self.inner.socket.shutdown().await?;
        Ok(())
    }
}

pub struct Bind<S> {
    inner: RequestInner<S>,
}

impl<S> Bind<S> {
    pub fn destination_address(&self) -> &TcpAddress {
        &self.inner.dest_address
    }

    pub fn user_id(&self) -> Bytes {
        self.inner.user_id.clone()
    }
}

pub struct Connected<S> {
    socket: S,
}

impl<S> AsyncRead for Connected<S>
where
    S: AsyncRead + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.socket).poll_read(cx, buf)
    }
}

impl<S> AsyncWrite for Connected<S>
where
    S: AsyncWrite + Unpin,
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

struct RequestInner<S> {
    socket: S,
    dest_address: TcpAddress,
    user_id: Bytes,
}

impl<S> RequestInner<S>
where
    S: AsyncWrite + Unpin,
{
    async fn send_reply(
        &mut self,
        reply: Reply,
        bind_address: Option<(Ipv4Addr, u16)>,
    ) -> Result<(), Error> {
        self.socket.write_u8(0).await?;
        self.socket.write_u8(reply.into()).await?;
        if let Some((ip, port)) = bind_address {
            self.socket.write_u16(port).await?;
            self.socket.write_all(&ip.octets()).await?;
        }
        else {
            self.socket.write_u16(0).await?;
            self.socket.write_u32(0).await?;
        }
        self.socket.flush().await?;
        Ok(())
    }
}
