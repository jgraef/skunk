use std::{
    future::Future,
    ops::Deref,
    pin::Pin,
    sync::Arc,
};

use tokio::io::{
    AsyncRead,
    AsyncWrite,
};

use super::Error;

pub trait Layer<S, T> {
    type Future: Future<Output = Result<(), Error>> + Send;

    fn layer(&self, source: S, target: T) -> Self::Future;
}

impl<S, T, F, Fut> Layer<S, T> for F
where
    F: Fn(S, T) -> Fut,
    Fut: Future<Output = Result<(), Error>> + Send + Sync,
{
    type Future = Fut;

    fn layer(&self, source: S, target: T) -> Fut {
        self(source, target)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Passthrough;

impl<S, T> Layer<S, T> for Passthrough
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    T: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Future = Pin<Box<dyn Future<Output = Result<(), Error>> + Send>>;

    fn layer(&self, mut source: S, mut target: T) -> Self::Future {
        Box::pin(async move {
            tokio::io::copy_bidirectional(&mut source, &mut target).await?;
            Ok(())
        })
    }
}

impl<L: Layer<S, T>, S, T> Layer<S, T> for Arc<L> {
    type Future = L::Future;

    fn layer(&self, source: S, target: T) -> Self::Future {
        self.deref().layer(source, target)
    }
}
