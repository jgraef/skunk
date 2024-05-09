use std::{string::FromUtf8Error, sync::Arc};

use serde::Deserialize;
use socks5_server::{connection::state::NeedAuthenticate, proto::{handshake::{password::{Request as PasswordRequest, Response as PasswordResponse}, Method}, Address, Reply}, Auth, Command, IncomingConnection, Server};
use tokio::{net::{TcpListener, TcpStream}, pin};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing_unwrap::ResultExt;

use super::Connect;

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

#[derive(Debug, Deserialize)]
pub struct SocksProxyConfig {
    address: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
}

pub async fn run(config: &SocksProxyConfig, shutdown: CancellationToken, connect: impl Connect) -> Result<(), Error> {
    let listener = TcpListener::bind((config.address.as_str(), config.port)).await?;
    let auth = match (&config.username, &config.password) {
        (None, None) => MaybeAuth::NoAuth,
        (Some(username), Some(password)) => MaybeAuth::Password {
            username: username.as_bytes().to_owned(),
            password: password.as_bytes().to_owned(),
        },
        _ => return Err(Error::AuthEitherBothOrNone),
    };

    let server = Server::new(listener, Arc::new(auth));

    loop {
        tokio::select! {
            result = server.accept() => {
                let (connection, address) = result?;
                let shutdown = shutdown.clone();
                let connect = connect.clone();
                tokio::spawn(async move {
                    let span = tracing::info_span!("connection", ?address);
                    let _guard = span.enter();
                    tokio::select!{
                        result = handle_connection(connection, connect) => {
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

async fn handle_connection(
    connection: IncomingConnection<Result<AuthResult, Error>, NeedAuthenticate>,
    connect: impl Connect,
) -> Result<(), Error> {
    let (connection, auth_result) = connection.authenticate().await
        .map_err(|(e, _)| e)?;
    match auth_result? {
        AuthResult::Ok => {
            tracing::info!("authenticated");
        },
        AuthResult::Failed => {
            tracing::info!("authentication failed");
            return Ok(())
        }
    }

    let command = connection.wait().await.map_err(|(e, _)| e)?;
    match command {
        Command::Associate(request, address) => {
            tracing::info!("associate not supported");
            let mut request = request.reply(Reply::CommandNotSupported, address).await.map_err(|(e, _)| e)?;
            request.close().await?;
            return Ok(());
        }
        Command::Bind(request, address) => {
            tracing::info!("bind not supported");
            let mut request = request.reply(Reply::CommandNotSupported, address).await.map_err(|(e, _)| e)?;
            request.close().await?;
            return Ok(());
        }
        Command::Connect(request, address) => {
            let target_address = match address.clone().try_into() {
                Ok(address) => address,
                Err(error) => {
                    tracing::error!("{error}");
                    let mut request = request.reply(Reply::AddressTypeNotSupported, address).await.map_err(|(e, _)| e)?;
                    request.close().await?;
                    return Ok(());
                }
            };

            let target = match connect.connect(target_address).await {
                Ok(connection) => connection,
                Err(error) => {
                    tracing::error!("{error}");
                    // todo: reply depending on error
                    let mut request = request.reply(Reply::ConnectionRefused, address).await.map_err(|(e, _)| e)?;
                    request.close().await?;
                    return Ok(());
                }
            };

            let mut request = request.reply(Reply::Succeeded, address).await.map_err(|(e, _)| e)?;
            pin!(target);
            tokio::io::copy_bidirectional(&mut request, &mut target).await?;
        },
        
    }
    
    Ok(())
}

#[derive(Debug, thiserror::Error)]
#[error("invalid hostname")]
pub struct InvalidHostname(#[from] FromUtf8Error);

impl TryFrom<Address> for super::Address {
    type Error = InvalidHostname;

    fn try_from(value: Address) -> Result<Self, Self::Error> {
        let address = match value {
            Address::SocketAddress(address) => Self::SocketAddress(address),
            Address::DomainAddress(hostname, port) => Self::DomainAddress { hostname: String::from_utf8(hostname)?, port, },
        };
        Ok(address)
    }
}

pub enum MaybeAuth {
    NoAuth,
    Password {
        username: Vec<u8>,
        password: Vec<u8>,
    }
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
            },
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