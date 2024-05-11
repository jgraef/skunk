use std::{
    future::Future,
    ops::Deref,
    sync::Arc,
};

use tokio::io::{
    AsyncRead,
    AsyncWrite,
};

use super::Error;

pub trait Layer<S, T> {
    fn layer(&self, source: S, target: T) -> impl Future<Output = Result<(), Error>> + Send;
}

impl<S, T, F, Fut> Layer<S, T> for F
where
    F: Fn(S, T) -> Fut,
    Fut: Future<Output = Result<(), Error>> + Send + Sync,
{
    fn layer(&self, source: S, target: T) -> impl Future<Output = Result<(), Error>> + Send {
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
    async fn layer(&self, mut source: S, mut target: T) -> Result<(), Error> {
        tokio::io::copy_bidirectional(&mut source, &mut target).await?;
        Ok(())
    }
}

impl<L: Layer<S, T>, S, T> Layer<S, T> for Arc<L> {
    fn layer(&self, source: S, target: T) -> impl Future<Output = Result<(), Error>> + Send {
        self.deref().layer(source, target)
    }
}
