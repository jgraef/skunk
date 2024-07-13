//! SOCKS server implementation.
//!
//! This provides a SOCKS4a/5 server that can be used to inspect traffic.

use std::{
    net::SocketAddr,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use tokio::{
    io::{
        AsyncRead,
        AsyncWrite,
        BufStream,
        ReadBuf,
    },
    net::{
        TcpListener,
        TcpStream,
    },
    sync::{
        mpsc,
        oneshot,
    },
};
use tracing::Instrument;

use super::{
    error::Error,
    v5::{
        server::{
            serve,
            AuthProvider,
            AuthResult,
            Connected,
            Request,
        },
        AuthMethod,
        RejectReason,
        SelectedAuthMethod,
        DEFAULT_PORT,
    },
};
use crate::{
    address::TcpAddress,
    proxy::DestinationAddress,
};

/// An incoming connection.
///
/// # Buffering
///
/// The underlying [`TcpStream`] is buffered (using a [`BufStream`]), so it's
/// necessary to call [`flush`] to make sure the written data is
/// actually sent.
///
/// [`flush`]: tokio::io::AsyncWriteExt::flush
#[derive(Debug)]
pub struct Incoming {
    inner: Connected<BufStream<TcpStream>, MaybeAuth>,
    destination_address: TcpAddress,
}

impl DestinationAddress for Incoming {
    fn destination_address(&self) -> &TcpAddress {
        &self.destination_address
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

/// Builder used to create a SOCKS server.
pub struct Builder {
    bind_address: SocketAddr,
    auth: MaybeAuth,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            bind_address: ([127, 0, 0, 1], DEFAULT_PORT).into(),
            auth: MaybeAuth::NoAuth,
        }
    }
}

impl Builder {
    /// Specify a bind address. Defaults to `127.0.0.1:9090`.
    pub fn with_bind_address(mut self, bind_address: impl Into<SocketAddr>) -> Self {
        self.bind_address = bind_address.into();
        self
    }

    /// Specify username and password for authentication. By default no
    /// authentication is used.
    ///
    /// # TODO
    ///
    /// This is not implemented yet.
    pub fn with_password(mut self, username: String, password: String) -> Self {
        self.auth = MaybeAuth::Password {
            username: username.into_bytes(),
            password: password.into_bytes(),
        };
        self
    }

    /// Listen for connection requests
    pub async fn listen(self) -> Result<ConnectionRequests, Error> {
        let listener = TcpListener::bind(&self.bind_address).await?;

        let (connection_requests_tx, connection_requests_rx) = mpsc::channel(16);

        tokio::spawn(async move {
            loop {
                let result = tokio::select! {
                    // terminate, if receiver half is dropped
                    _ = connection_requests_tx.closed() => break,

                    // wait for connection (or error)
                    result = listener.accept() => result,
                };

                match result {
                    Ok((connection, address)) => {
                        let span = tracing::info_span!("socks", %address);
                        let connection_requests_tx = connection_requests_tx.clone();
                        let auth = self.auth.clone();

                        tokio::spawn(
                            async move {
                                if let Err(e) =
                                    handle_connection(connection, auth, connection_requests_tx)
                                        .await
                                {
                                    tracing::error!("{e}");
                                }
                            }
                            .instrument(span),
                        );
                    }
                    Err(e) => {
                        let _ = connection_requests_tx.send(Err(e.into())).await;
                        break;
                    }
                }
            }
        });

        Ok(ConnectionRequests {
            connection_requests_rx,
        })
    }
}

/// Stream of connection requests
#[derive(Debug)]
pub struct ConnectionRequests {
    connection_requests_rx: mpsc::Receiver<Result<ConnectionRequest, Error>>,
}

impl ConnectionRequests {
    pub async fn next(&mut self) -> Result<ConnectionRequest, Error> {
        if let Some(result) = self.connection_requests_rx.recv().await {
            result
        }
        else {
            Err(Error::Io(std::io::ErrorKind::NotConnected.into()))
        }
    }
}

/// A request to connect to a destination address
///
/// Either [`accept`] or [`reject`] the request, taking into account the
/// [`destination_address`]. If this is dropped, the request will be rejected
/// with a generic reason.
///
/// [`accept`]: [Self::accept]
/// [`reject`]: [Self::reject]
/// [`destination_address`]: [Self::destination_address]
#[derive(Debug)]
pub struct ConnectionRequest {
    destination_address: TcpAddress,
    ack_tx: oneshot::Sender<Result<TcpAddress, RejectReason>>,
    connection_rx: oneshot::Receiver<Result<Incoming, Error>>,
}

impl ConnectionRequest {
    pub fn destination_address(&self) -> &TcpAddress {
        &self.destination_address
    }

    pub async fn accept(self, bind_address: TcpAddress) -> Result<Incoming, Error> {
        let _ = self.ack_tx.send(Ok(bind_address));
        let connection = self
            .connection_rx
            .await
            .expect("connection_tx dropped without error")?;
        Ok(connection)
    }

    pub fn reject(self, reason: impl Into<Option<RejectReason>>) {
        let _ = self.ack_tx.send(Err(reason
            .into()
            .unwrap_or(RejectReason::ConnectionRefused)));
    }
}

/// Handle a single connection
async fn handle_connection(
    connection: TcpStream,
    auth: MaybeAuth,
    connection_requests_tx: mpsc::Sender<Result<ConnectionRequest, Error>>,
) -> Result<(), Error> {
    let connection = BufStream::new(connection);
    let request = serve(connection, &auth).await?;

    match request {
        Request::Associate(request) => request.reject(RejectReason::CommandNotSupported).await?,
        Request::Bind(request) => request.reject(RejectReason::CommandNotSupported).await?,
        Request::Connect(request) => {
            let destination_address = request.destination_address().clone();

            let (ack_tx, ack_rx) = oneshot::channel();
            let (connection_tx, connection_rx) = oneshot::channel();

            // doesn't matter if receiver was dropped, since the ACK will fail
            let _ = connection_requests_tx
                .send(Ok(ConnectionRequest {
                    destination_address: destination_address.clone(),
                    ack_tx,
                    connection_rx,
                }))
                .await;

            match ack_rx.await {
                Ok(Ok(bind_address)) => {
                    // connection request accepted
                    let result = request.accept(&bind_address).await.map(|connection| {
                        Incoming {
                            inner: connection,
                            destination_address,
                        }
                    });
                    let _ = connection_tx.send(result);
                }
                Ok(Err(reason)) => {
                    // connection request rejected with reason
                    request.reject(reason).await?;
                }
                Err(_) => {
                    // ACK sender dropped
                    request.reject(RejectReason::ConnectionRefused).await?;
                }
            }
        }
    }

    Ok(())
}

/// Authentication configuration.
#[derive(Clone, Debug)]
pub enum MaybeAuth {
    NoAuth,
    Password {
        username: Vec<u8>,
        password: Vec<u8>,
    },
}

impl AuthProvider for MaybeAuth {
    type Data = ();
    type Socket<S> = S
    where S: AsyncRead + AsyncWrite + Unpin;

    fn select_method(&self, methods: &[AuthMethod]) -> SelectedAuthMethod {
        let accept = match self {
            Self::NoAuth => AuthMethod::NoAuthentication,
            Self::Password { .. } => AuthMethod::UsernamePassword,
        };
        if methods.iter().any(|m| *m == accept) {
            accept.into()
        }
        else {
            SelectedAuthMethod::NoAcceptable
        }
    }

    async fn authenticate<S>(
        &self,
        _auth_method: AuthMethod,
        socket: S,
    ) -> Result<AuthResult<S, Self>, Error>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let accept = match self {
            MaybeAuth::NoAuth => true,
            MaybeAuth::Password {
                username: _,
                password: _,
            } => {
                todo!("Implement SOCKS authentication");
            }
        };

        let result = if accept {
            AuthResult::Success { socket, data: () }
        }
        else {
            AuthResult::Failed { socket }
        };

        Ok(result)
    }
}
