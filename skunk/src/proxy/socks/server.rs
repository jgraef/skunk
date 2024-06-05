//! SOCKS server implementation.
//!
//! This provides a SOCKS4a/5 server that can be used to inspect traffic.

use std::{
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
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
};
use tokio_util::sync::CancellationToken;
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
    connect::{
        Connect,
        ConnectTcp,
    },
    proxy::{
        DestinationAddress,
        Passthrough,
        Proxy,
    },
    util::error::ResultExt,
};

/// An incoming connection.
///
/// # Buffering
///
/// The underlying [`TcpStream`] is buffered (using a [`BufStream`]), so it's
/// necessary to call [`AsyncWrite::flush`] to make sure the written data is
/// actually sent.
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
    /// Specify a bind address. Defaults to `127.0.0.1:9090`.
    pub fn with_bind_address(mut self, bind_address: impl Into<SocketAddr>) -> Self {
        self.bind_address = bind_address.into();
        self
    }

    /// Pass in a [`CancellationToken`] that can be used to shutdown the server.
    pub fn with_graceful_shutdown(mut self, shutdown: CancellationToken) -> Self {
        self.shutdown = shutdown;
        self
    }

    /// Specify username and password for authentication. By default no
    /// authentication is used.
    pub fn with_password(mut self, username: String, password: String) -> Self {
        self.auth = MaybeAuth::Password {
            username: username.into_bytes(),
            password: password.into_bytes(),
        };
        self
    }

    /// Specify a [connector][Connect]. This is used to establish connections to
    /// the destination as requested by the proxy client. By default
    /// [`ConnectTcp`] will be used.
    pub fn with_connect<C2>(self, connect: C2) -> Builder<C2, P>
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

    /// Specify a [`Proxy`] that is used to forward incoming data to the
    /// destination and vice-versa. By default this uses [`Passthrough`] that
    /// just blindly passes through the data without inspecting or altering it.
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

    /// Run the server.
    pub async fn serve(self) -> Result<(), Error>
    where
        C: Connect + Clone + Send + 'static,
        P: Proxy<Incoming, C::Connection> + Clone + Send + 'static,
    {
        run(
            self.bind_address,
            self.shutdown,
            Arc::new(self.auth),
            self.connect,
            self.proxy,
        )
        .await?;
        Ok(())
    }
}

/// Function that actually runs the server.
async fn run<C, P>(
    bind_address: SocketAddr,
    shutdown: CancellationToken,
    auth: Arc<MaybeAuth>,
    connect: C,
    proxy: P,
) -> Result<(), Error>
where
    P: Proxy<Incoming, C::Connection> + Clone + Send + 'static,
    C: Connect + Clone + Send + 'static,
{
    // todo: this should take a `crate::accept::Listen`

    let listener = TcpListener::bind(bind_address).await?;

    loop {
        tokio::select! {
            result = listener.accept() => {
                let (connection, address) = result?;

                let auth = auth.clone();
                let shutdown = shutdown.clone();
                let connect = connect.clone();
                let proxy = proxy.clone();

                let span = tracing::info_span!("socks", ?address);

                tokio::spawn(async move {
                    tokio::select!{
                        result = handle_connection(connection, auth, connect, proxy) => {
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

/// Handle a single connection
async fn handle_connection<C, P>(
    connection: TcpStream,
    auth: Arc<MaybeAuth>,
    connect: C,
    proxy: P,
) -> Result<(), Error>
where
    C: Connect + Clone + Send + 'static,
    P: Proxy<Incoming, C::Connection> + Clone + Send + 'static,
{
    let connection = BufStream::new(connection);
    let request = serve(connection, auth.as_ref()).await?;

    match request {
        Request::Associate(request) => request.reject(RejectReason::CommandNotSupported).await?,
        Request::Bind(request) => request.reject(RejectReason::CommandNotSupported).await?,
        Request::Connect(request) => {
            let destination_address = request.destination_address().clone();

            // connect to destination
            let outgoing = match connect.connect(&destination_address).await {
                Ok(connection) => connection,
                Err(error) => {
                    tracing::error!("{error}");
                    // todo: reply depending on error
                    request.reject(RejectReason::ConnectionRefused).await?;
                    return Ok(());
                }
            };

            // send reply, that we successfully connected to the target
            // todo: actually give it the bind address
            let incoming = request.accept(&destination_address).await?;

            // run proxy
            let _ = proxy
                .proxy(
                    Incoming {
                        inner: incoming,
                        destination_address,
                    },
                    outgoing,
                )
                .await
                .log_error_with_message("Layer returned an error");
        }
    }

    Ok(())
}

/// Authentication configuration.
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
        methods
            .iter()
            .any(|m| *m == accept)
            .then_some(accept.into())
            .unwrap_or(SelectedAuthMethod::NoAcceptable)
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
                todo!();
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
