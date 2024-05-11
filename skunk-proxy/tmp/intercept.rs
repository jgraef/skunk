use std::{
    fmt::Display,
    num::NonZeroUsize,
    sync::{
        atomic::{
            AtomicUsize,
            Ordering,
        },
        Arc,
        RwLock,
    },
};

use axum::async_trait;
use tokio::io::{
    AsyncRead,
    AsyncWrite,
};

use super::{
    connect::Connect, filter::{FilterId, Filters}, forward::{
        Forward,
        ForwardTo,
        Passthrough,
    }, TcpAddress
};
use crate::app::tls::TlsContext;


#[derive(Clone)]
struct Intercept<C> {
    tls: TlsContext,
    filters: Arc<RwLock<Filters>>,
    connect: C,
}

#[async_trait]
impl<C: Connect> Forward for Intercept<C> {
    type ForwardTo = MaybeIntercepted<C::Connection>;

    async fn forward_connect(
        &self,
        address: &TcpAddress,
    ) -> Result<Self::ForwardTo, std::io::Error> {
        let match_id = {
            let filters = self.filters.read().unwrap();
            filters.matches(address)
        };

        // todo: log errors
        let target = self.connect.connect(address).await?;

        let target = if let Some(match_id) = match_id {
            MaybeIntercepted::Intercepted(Intercepted {
                target,
                address: address.clone(),
                match_id,
                tls: self.tls.clone(),
            })
        }
        else {
            MaybeIntercepted::Passthrough(Passthrough::new(target))
        };

        Ok(target)
    }
}

pub enum MaybeIntercepted<T> {
    Intercepted(Intercepted<T>),
    Passthrough(Passthrough<T>),
}

#[async_trait]
impl<T: AsyncRead + AsyncWrite + Send + Sync + Unpin> ForwardTo for MaybeIntercepted<T> {
    async fn forward<S: AsyncRead + AsyncWrite + Send + Sync + Unpin>(
        self,
        source: S,
    ) -> Result<(), std::io::Error> {
        match self {
            Self::Intercepted(intercepted) => {
                intercepted.forward(source).await?;
            }
            Self::Passthrough(passthrough) => {
                passthrough.forward(source).await?;
            }
        }
        Ok(())
    }
}

pub struct Intercepted<T> {
    target: T,
    address: TcpAddress,
    match_id: FilterId,
    tls: TlsContext,
}

#[async_trait]
impl<T: AsyncRead + AsyncWrite + Send + Sync + Unpin> ForwardTo for Intercepted<T> {
    async fn forward<S: AsyncRead + AsyncWrite + Send + Sync + Unpin>(
        self,
        source: S,
    ) -> Result<(), std::io::Error> {
        // todo:
        // 1. unwrap TLS:
        //  - wrap source as TlsServer stream
        //  - wrap target as TlsClient stream
        // 2. unwrap http stream
        //

        let source = self.tls.accept(source, &self.address.host).await?;

        let target = self.tls.connect(self.target, &self.address.host).await?;

        todo!();
    }
}

pub enum MaybeTlsServerStream<S> {
    Tls(tokio_rustls::server::TlsStream<S>),
    Plaintext(S),
}

impl<S> MaybeTlsServerStream<S> {}

pub enum MaybeTlsClientStream<S> {
    Tls(tokio_rustls::client::TlsStream<S>),
    Plaintext(S),
}
