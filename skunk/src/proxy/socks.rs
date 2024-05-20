use std::{
    net::SocketAddr,
    pin::Pin,
    string::FromUtf8Error,
    sync::Arc,
    task::{
        Context,
        Poll,
    },
};

use async_trait::async_trait;
use socks5_server::{
    connection::state::NeedAuthenticate,
    proto::{
        handshake::{
            password::{
                Request as PasswordRequest,
                Response as PasswordResponse,
            },
            Method,
        },
        Address,
        Reply,
    },
    Auth,
    Command,
    IncomingConnection,
    Server,
};
use tokio::{
    io::{
        AsyncRead,
        AsyncWrite,
        ReadBuf,
    },
    net::{
        TcpListener,
        TcpStream,
    },
};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use super::{
    Passthrough,
    Proxy,
    TargetAddress,
};
use crate::{
    address::{
        HostAddress,
        TcpAddress,
    },
    connect::{
        Connect,
        ConnectTcp,
    },
    util::error::ResultExt,
};

pub const DEFAULT_PORT: u16 = 9090;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("Error during authentication")]
    Password(#[from] socks5_server::proto::handshake::password::Error),

    #[error("protocol error")]
    Protocol(#[from] socks5_server::proto::Error),
}

pub struct Incoming {
    inner: socks5_server::Connect<socks5_server::connection::connect::state::Ready>,
    target_address: TcpAddress,
}

impl TargetAddress for Incoming {
    fn target_address(&self) -> &TcpAddress {
        &self.target_address
    }
}

impl AsyncRead for Incoming {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for Incoming {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

pub struct Builder<C = ConnectTcp, P = Passthrough> {
    bind_address: SocketAddr,
    shutdown: CancellationToken,
    auth: MaybeAuth,
    connect: C,
    proxy: P,
}

impl<C: Default, P: Default> Default for Builder<C, P> {
    fn default() -> Self {
        Self {
            bind_address: ([127, 0, 0, 1], DEFAULT_PORT).into(),
            shutdown: Default::default(),
            auth: MaybeAuth::NoAuth,
            connect: Default::default(),
            proxy: Default::default(),
        }
    }
}

impl<C, P> Builder<C, P> {
    pub fn with_bind_address(mut self, bind_address: impl Into<SocketAddr>) -> Self {
        self.bind_address = bind_address.into();
        self
    }

    pub fn with_graceful_shutdown(mut self, shutdown: CancellationToken) -> Self {
        self.shutdown = shutdown;
        self
    }

    pub fn with_password(mut self, username: String, password: String) -> Self {
        self.auth = MaybeAuth::Password {
            username: username.into_bytes(),
            password: password.into_bytes(),
        };
        self
    }

    pub fn with_handler<C2>(self, connect: C2) -> Builder<C2, P>
    where
        C2: Connect + Clone + Send + 'static,
    {
        Builder {
            bind_address: self.bind_address,
            shutdown: self.shutdown,
            auth: self.auth,
            connect,
            proxy: self.proxy,
        }
    }

    pub fn with_proxy<P2>(self, proxy: P2) -> Builder<C, P2>
    where
        C: Connect,
        P2: Proxy<Incoming, C::Connection> + Clone + Send + 'static,
    {
        Builder {
            bind_address: self.bind_address,
            shutdown: self.shutdown,
            auth: self.auth,
            connect: self.connect,
            proxy,
        }
    }

    pub async fn serve(self) -> Result<(), Error>
    where
        C: Connect + Clone + Send + 'static,
        P: Proxy<Incoming, C::Connection> + Clone + Send + 'static,
    {
        run(
            self.bind_address,
            self.shutdown,
            self.auth,
            self.connect,
            self.proxy,
        )
        .await?;
        Ok(())
    }
}

async fn run<C, P>(
    bind_address: SocketAddr,
    shutdown: CancellationToken,
    auth: MaybeAuth,
    connect: C,
    proxy: P,
) -> Result<(), Error>
where
    P: Proxy<Incoming, C::Connection> + Clone + Send + 'static,
    C: Connect + Clone + Send + 'static,
{
    // todo: this should take a `crate::accept::Listen`, but that's impossible with
    // the socks5_server crate.

    let listener = TcpListener::bind(bind_address).await?;
    let server = Server::new(listener, Arc::new(auth));

    loop {
        tokio::select! {
            result = server.accept() => {
                let (connection, address) = result?;
                let shutdown = shutdown.clone();
                let connect = connect.clone();
                let proxy = proxy.clone();
                let span = tracing::info_span!("socks", ?address);

                tokio::spawn(async move {
                    tokio::select!{
                        result = handle_connection(connection, connect, proxy) => {
                            let _ = result.log_error();
                        },
                        _ = shutdown.cancelled() => {},
                    }
                }.instrument(span));
            },
            _ = shutdown.cancelled() => {
                break;
            },
        }
    }

    Ok(())
}

async fn handle_connection<C, P>(
    connection: IncomingConnection<Result<AuthResult, Error>, NeedAuthenticate>,
    connect: C,
    proxy: P,
) -> Result<(), Error>
where
    C: Connect + Clone + Send + 'static,
    P: Proxy<Incoming, C::Connection> + Clone + Send + 'static,
{
    let (connection, auth_result) = connection.authenticate().await.map_err(|(e, _)| e)?;
    match auth_result? {
        AuthResult::Ok => {
            tracing::trace!("authenticated");
        }
        AuthResult::Failed => {
            tracing::warn!("authentication failed");
            return Ok(());
        }
    }

    let command = connection.wait().await.map_err(|(e, _)| e)?;
    match command {
        Command::Associate(request, address) => {
            tracing::info!("associate not supported");
            let mut request = request
                .reply(Reply::CommandNotSupported, address)
                .await
                .map_err(|(e, _)| e)?;
            request.close().await?;
            return Ok(());
        }
        Command::Bind(request, address) => {
            tracing::info!("bind not supported");
            let mut request = request
                .reply(Reply::CommandNotSupported, address)
                .await
                .map_err(|(e, _)| e)?;
            request.close().await?;
            return Ok(());
        }
        Command::Connect(request, address) => {
            let target_address = match address.clone().try_into() {
                Ok(address) => address,
                Err(error) => {
                    tracing::error!("{error}");
                    let mut request = request
                        .reply(Reply::AddressTypeNotSupported, address)
                        .await
                        .map_err(|(e, _)| e)?;
                    request.close().await?;
                    return Ok(());
                }
            };

            // connect to target
            let target = match connect.connect(&target_address).await {
                Ok(connection) => connection,
                Err(error) => {
                    tracing::error!("{error}");
                    // todo: reply depending on error
                    let mut request = request
                        .reply(Reply::ConnectionRefused, address)
                        .await
                        .map_err(|(e, _)| e)?;
                    request.close().await?;
                    return Ok(());
                }
            };

            // send reply, that we successfully connected to the target
            let source = request
                .reply(Reply::Succeeded, address)
                .await
                .map(|inner| {
                    Incoming {
                        inner,
                        target_address,
                    }
                })
                .map_err(|(e, _)| e)?;

            // run layer
            // we pass mutable references, so we can shut down the streams properly
            // afterwards
            let _ = proxy
                .proxy(source, target)
                .await
                .log_error_with_message("Layer returned an error");

            // shut down streams. this flushes any buffered data
            // note: sometimes the stream is already closed. i think we can just
            // ignore the errors.
            //let _ = target.shutdown().await;
            //let _ = source.shutdown().await;
        }
    }

    Ok(())
}

#[derive(Debug, thiserror::Error)]
#[error("invalid hostname")]
pub struct InvalidHostname(#[from] FromUtf8Error);

impl TryFrom<Address> for TcpAddress {
    type Error = InvalidHostname;

    fn try_from(value: Address) -> Result<Self, Self::Error> {
        let address = match value {
            Address::SocketAddress(address) => {
                Self {
                    host: HostAddress::IpAddress(address.ip()),
                    port: address.port(),
                }
            }
            Address::DomainAddress(hostname, port) => {
                Self {
                    host: HostAddress::DnsName(String::from_utf8(hostname)?),
                    port,
                }
            }
        };
        Ok(address)
    }
}

pub enum MaybeAuth {
    NoAuth,
    Password {
        username: Vec<u8>,
        password: Vec<u8>,
    },
}

#[async_trait]
impl Auth for MaybeAuth {
    type Output = Result<AuthResult, Error>;

    fn as_handshake_method(&self) -> Method {
        match self {
            MaybeAuth::NoAuth => Method::NONE,
            MaybeAuth::Password { .. } => Method::PASSWORD,
        }
    }

    async fn execute(&self, stream: &mut TcpStream) -> Self::Output {
        match self {
            MaybeAuth::NoAuth => Ok(AuthResult::Ok),
            MaybeAuth::Password { username, password } => {
                let req = PasswordRequest::read_from(stream).await?;
                let correct = &req.username == username && &req.password == password;
                let resp = PasswordResponse::new(true);
                resp.write_to(stream).await?;
                Ok(correct.into())
            }
        }
    }
}

pub enum AuthResult {
    Ok,
    Failed,
}

impl From<bool> for AuthResult {
    fn from(value: bool) -> Self {
        value.then_some(Self::Ok).unwrap_or(Self::Failed)
    }
}
