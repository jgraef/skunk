use std::{
    future::Future,
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

impl<L: Layer<S, T>, S, T> Layer<S, T> for &L {
    type Output = L::Output;

    fn layer(&self, source: S, target: T) -> impl Future<Output = Result<L::Output, Error>> + Send {
        (*self).layer(source, target)
    }
}

impl<L: Layer<S, T>, S, T> Layer<S, T> for &mut L {
    type Output = L::Output;

    fn layer(&self, source: S, target: T) -> impl Future<Output = Result<L::Output, Error>> + Send {
        (self as &L).layer(source, target)
    }
}

impl<L: Layer<S, T>, S, T> Layer<S, T> for Arc<L> {
    type Output = L::Output;

    fn layer(&self, source: S, target: T) -> impl Future<Output = Result<L::Output, Error>> + Send {
        self.as_ref().layer(source, target)
    }
}

pub struct FnLayer<Func>(Func);

impl<Source, Target, Func, Fut, Output> Layer<Source, Target> for FnLayer<Func>
where
    Func: Fn(Source, Target) -> Fut,
    Fut: Future<Output = Result<Output, Error>> + Send + Sync,
{
    type Output = Output;

    fn layer(
        &self,
        source: Source,
        target: Target,
    ) -> impl Future<Output = Result<Output, Error>> + Send {
        (self.0)(source, target)
    }
}

pub fn fn_layer<Func>(function: Func) -> FnLayer<Func> {
    FnLayer(function)
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
