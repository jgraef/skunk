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
        AsyncWriteExt,
        ReadBuf,
    },
    net::{
        TcpListener,
        TcpStream,
    },
};
use tokio_util::sync::CancellationToken;
use tracing_unwrap::ResultExt;

use super::ProxySource;
use crate::{
    address::{
        HostAddress,
        TcpAddress,
    },
    connect::Connect,
    layer::Layer,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("Either both username and password or neither must be specified")]
    AuthEitherBothOrNone,

    #[error("Error during authentication")]
    Password(#[from] socks5_server::proto::handshake::password::Error),

    #[error("protocol error")]
    Protocol(#[from] socks5_server::proto::Error),
}

pub struct SocksSource {
    inner: socks5_server::Connect<socks5_server::connection::connect::state::Ready>,
    target_address: TcpAddress,
}

impl ProxySource for SocksSource {
    fn target_address(&self) -> &TcpAddress {
        &self.target_address
    }
}

impl AsyncRead for SocksSource {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for SocksSource {
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

pub struct Builder {
    bind_address: SocketAddr,
    shutdown: Option<CancellationToken>,
    auth: Option<(String, String)>,
}

impl Builder {
    pub fn new(bind_address: impl Into<SocketAddr>) -> Self {
        Self {
            bind_address: bind_address.into(),
            shutdown: None,
            auth: None,
        }
    }

    pub fn with_graceful_shutdown(mut self, shutdown: CancellationToken) -> Self {
        self.shutdown = Some(shutdown);
        self
    }

    pub fn with_password(mut self, username: String, password: String) -> Self {
        self.auth = Some((username, password));
        self
    }

    pub async fn serve<C, L>(self, connect: C, layer: L) -> Result<(), Error>
    where
        C: Connect + Clone + Send + 'static,
        L: for<'s, 't> Layer<&'s mut SocksSource, &'t mut <C as Connect>::Connection>
            + Clone
            + Send
            + 'static,
    {
        let auth = self.auth.map_or(MaybeAuth::NoAuth, |(username, password)| {
            MaybeAuth::Password {
                username: username.into_bytes(),
                password: password.into_bytes(),
            }
        });
        run(
            self.bind_address,
            self.shutdown.unwrap_or_default(),
            auth,
            connect,
            layer,
        )
        .await?;
        Ok(())
    }
}

async fn run<C, L>(
    bind_address: SocketAddr,
    shutdown: CancellationToken,
    auth: MaybeAuth,
    connect: C,
    layer: L,
) -> Result<(), Error>
where
    C: Connect + Clone + Send + 'static,
    L: for<'s, 't> Layer<&'s mut SocksSource, &'t mut <C as Connect>::Connection>
        + Clone
        + Send
        + 'static,
{
    let listener = TcpListener::bind(bind_address).await?;
    let server = Server::new(listener, Arc::new(auth));

    loop {
        tokio::select! {
            result = server.accept() => {
                let (connection, address) = result?;
                let shutdown = shutdown.clone();
                let connect = connect.clone();
                let layer = layer.clone();

                tokio::spawn(async move {
                    let span = tracing::info_span!("connection", ?address);
                    let _guard = span.enter();
                    tokio::select!{
                        result = handle_connection(connection, connect, layer) => {
                            result.ok_or_log();
                        },
                        _ = shutdown.cancelled() => {},
                    }
                });
            },
            _ = shutdown.cancelled() => {},
        }
    }
}

async fn handle_connection<C, L>(
    connection: IncomingConnection<Result<AuthResult, Error>, NeedAuthenticate>,
    connect: C,
    layer: L,
) -> Result<(), Error>
where
    C: Connect,
    L: for<'s, 't> Layer<&'s mut SocksSource, &'t mut <C as Connect>::Connection>,
{
    let (connection, auth_result) = connection.authenticate().await.map_err(|(e, _)| e)?;
    match auth_result? {
        AuthResult::Ok => {
            tracing::info!("authenticated");
        }
        AuthResult::Failed => {
            tracing::info!("authentication failed");
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
            let mut target = match connect.connect(&target_address).await {
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
            let mut source = request
                .reply(Reply::Succeeded, address)
                .await
                .map(|inner| {
                    SocksSource {
                        inner,
                        target_address,
                    }
                })
                .map_err(|(e, _)| e)?;

            // run layer
            // we pass mutable references, so we can shut down the streams properly
            // afterwards
            layer
                .layer(&mut source, &mut target)
                .await
                .expect("todo: handle error");

            // shut down streams. this flushes any buffered data
            target.shutdown().await?;
            source.shutdown().await?;
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
            MaybeAuth::NoAuth => todo!(),
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
