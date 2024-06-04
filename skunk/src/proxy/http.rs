use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::Arc,
};

use http_body_util::Empty;
use hyper::{
    body::Incoming,
    service::service_fn,
    Method,
    Request,
    Response,
    StatusCode,
};
use hyper_util::rt::TokioIo;
use tokio::net::{
    TcpListener,
    TcpStream,
};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use super::{
    Passthrough,
    Proxy,
};
use crate::{
    connect::{
        Connect,
        ConnectTcp,
    },
    util::error::ResultExt,
};

pub const DEFAULT_PORT: u16 = 8080;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("hyper error")]
    Hyper(#[from] hyper::Error),
}

pub struct Builder<C = ConnectTcp, P = Passthrough> {
    bind_address: SocketAddr,
    shutdown: CancellationToken,
    #[cfg(feature = "tls")]
    tls_client_config: Option<Arc<rustls::ClientConfig>>,
    connect: C,
    proxy: P,
}

impl<C: Default, P: Default> Default for Builder<C, P> {
    fn default() -> Self {
        Self {
            bind_address: ([127, 0, 0, 1], DEFAULT_PORT).into(),
            shutdown: Default::default(),
            #[cfg(feature = "tls")]
            tls_client_config: None,
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

    #[cfg(feature = "tls")]
    pub fn with_tls_client(
        mut self,
        tls_client_config: impl Into<Arc<rustls::ClientConfig>>,
    ) -> Self {
        self.tls_client_config = Some(tls_client_config.into());
        self
    }

    pub fn with_connect<C2>(self, connect: C2) -> Builder<C2, P> {
        Builder {
            bind_address: self.bind_address,
            shutdown: self.shutdown,
            #[cfg(feature = "tls")]
            tls_client_config: self.tls_client_config,
            connect,
            proxy: self.proxy,
        }
    }

    pub fn with_proxy<P2>(self, proxy: P2) -> Builder<C, P2> {
        Builder {
            bind_address: self.bind_address,
            shutdown: self.shutdown,
            #[cfg(feature = "tls")]
            tls_client_config: self.tls_client_config,
            connect: self.connect,
            proxy,
        }
    }

    pub async fn serve(self) -> Result<(), Error>
    where
        C: Connect + Clone + Send + 'static,
        P: HttpProxy<Incoming, C::Connection> + Clone + Send + 'static,
    {
        run(self.bind_address, self.shutdown, self.connect, self.layer).await?;
        Ok(())
    }
}

async fn run<C, P>(
    bind_address: SocketAddr,
    shutdown: CancellationToken,
    connect: C,
    proxy: P,
) -> Result<(), Error>
where
    C: Connect + Clone + Send + 'static,
    P: HttpProxy<Incoming, C::Connection> + Clone + Send + 'static,
{
    let listener = TcpListener::bind(bind_address).await?;

    loop {
        tokio::select! {
            result = listener.accept() => {
                let (connection, address) = result?;
                let shutdown = shutdown.clone();
                let connect = connect.clone();
                let proxy = proxy.clone();
                let span = tracing::info_span!("http-proxy", ?address);

                tokio::spawn(async move {
                    tokio::select!{
                        result = handle_connection(connection, connect, proxy) => {
                            let _ = result.log_error();
                        },
                        _ = shutdown.cancelled() => {},
                    }
                }.instrument(span));
            },
            _ = shutdown.cancelled() => {},
        }
    }
}

async fn handle_connection<C, P>(connection: TcpStream, connect: C, proxy: P) -> Result<(), Error>
where
    C: Connect + Clone + Send + 'static,
    P: HttpProxy<Incoming, C::Connection> + Clone + Send + 'static,
{
    hyper::server::conn::http1::Builder::new()
        .serve_connection(
            TokioIo::new(connection),
            service_fn(move |request: Request<Incoming>| {
                let _connect = connect.clone();
                let _layer = layer.clone();
                async move {
                    match request.method() {
                        &Method::CONNECT => {
                            tokio::spawn(async move {
                                let _upgraded = hyper::upgrade::on(request).await?;
                                Ok::<(), Error>(())
                            });

                            let response = Response::builder()
                                .status(StatusCode::SWITCHING_PROTOCOLS)
                                .body(Empty::<&[u8]>::new())
                                .expect("build invalid response");

                            Ok::<_, Infallible>(response)
                        }
                        _ => {
                            todo!();
                        }
                    }
                }
            }),
        )
        .await?;
    Ok(())
}

pub trait HttpProxy<O>: Proxy<ConnectStream, O> {}
