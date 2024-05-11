use std::{
    pin::Pin,
    string::FromUtf8Error,
    sync::Arc,
    task::{
        Context,
        Poll,
    },
};

use async_trait::async_trait;
use serde::Deserialize;
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
use tracing_unwrap::ResultExt;

use crate::core::{
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

pub struct SocksStream {
    inner: socks5_server::Connect<socks5_server::connection::connect::state::Ready>,
}

impl AsyncRead for SocksStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self).poll_read(cx, buf)
    }
}

impl AsyncWrite for SocksStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        Pin::new(&mut self).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self).poll_shutdown(cx)
    }
}

#[derive(Debug, Deserialize)]
pub struct SocksProxyConfig {
    address: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
}

pub async fn run<C, L>(
    config: &SocksProxyConfig,
    shutdown: CancellationToken,
    connect: C,
    layer: L,
) -> Result<(), Error>
where
    C: Connect + Clone + Send + 'static,
    L: Layer<SocksStream, <C as Connect>::Connection> + Clone + Send + 'static,
{
    let listener = TcpListener::bind((config.address.as_str(), config.port)).await?;
    let auth = match (&config.username, &config.password) {
        (None, None) => MaybeAuth::NoAuth,
        (Some(username), Some(password)) => {
            MaybeAuth::Password {
                username: username.as_bytes().to_owned(),
                password: password.as_bytes().to_owned(),
            }
        }
        _ => return Err(Error::AuthEitherBothOrNone),
    };

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
    L: Layer<SocksStream, <C as Connect>::Connection>,
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

            let connection = match connect.connect(&target_address).await {
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

            let socks_stream = request
                .reply(Reply::Succeeded, address)
                .await
                .map(|inner| SocksStream { inner })
                .map_err(|(e, _)| e)?;

            layer
                .layer(socks_stream, connection)
                .await
                .expect("todo: handle error");
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
