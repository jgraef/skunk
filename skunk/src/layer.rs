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
    type Output;

    fn layer(
        &self,
        source: S,
        target: T,
    ) -> impl Future<Output = Result<Self::Output, Error>> + Send;
}

impl<S, T, F, Fut, O> Layer<S, T> for F
where
    F: Fn(S, T) -> Fut,
    Fut: Future<Output = Result<O, Error>> + Send + Sync,
{
    type Output = O;

    fn layer(&self, source: S, target: T) -> impl Future<Output = Result<O, Error>> + Send {
        self(source, target)
    }
}

// todo: move or rename. this is specific to an AsyncRead/Write layer
#[derive(Clone, Copy, Debug)]
pub struct Passthrough;

impl<S, T> Layer<S, T> for Passthrough
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    T: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Output = ();

    async fn layer(&self, mut source: S, mut target: T) -> Result<(), Error> {
        tokio::io::copy_bidirectional(&mut source, &mut target).await?;
        Ok(())
    }
}

impl<L: Layer<S, T>, S, T> Layer<S, T> for Arc<L> {
    type Output = L::Output;

    fn layer(&self, source: S, target: T) -> impl Future<Output = Result<L::Output, Error>> + Send {
        self.deref().layer(source, target)
    }
}

pub struct Conditional<Cond, Then, Else> {
    pub condition: Cond,
    pub then_layer: Then,
    pub else_layer: Else,
}

impl<Cond, Then, Else, Source, Target> Layer<Source, Target> for Conditional<Cond, Then, Else>
where
    Source: Send,
    Target: Send,
    Cond:
        for<'source, 'target> Layer<&'source Source, &'target Target, Output = bool> + Send + Sync,
    Then: Layer<Source, Target> + Send + Sync,
    Else: Layer<Source, Target, Output = Then::Output> + Send + Sync,
{
    type Output = Then::Output;

    async fn layer(&self, source: Source, target: Target) -> Result<Then::Output, Error> {
        let output = if self.condition.layer(&source, &target).await? {
            self.then_layer.layer(source, target).await?
        }
        else {
            self.else_layer.layer(source, target).await?
        };
        Ok(output)
    }
}
