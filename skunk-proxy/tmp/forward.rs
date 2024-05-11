use std::pin::Pin;

use async_trait::async_trait;
use tokio::io::{
    AsyncRead,
    AsyncWrite,
};

use super::{
    connect::Connect, layer::Layer, TcpAddress
};

#[async_trait]
pub trait ForwardTo: Send + Sync {
    async fn forward<S: AsyncRead + AsyncWrite + Send + Sync + Unpin>(
        self,
        source: S,
    ) -> Result<(), std::io::Error>;
}

#[async_trait]
pub trait Forward: Send + Sync {
    type ForwardTo: ForwardTo;

    async fn forward_connect(
        &self,
        address: &TcpAddress,
    ) -> Result<Self::ForwardTo, std::io::Error>;
}

#[async_trait]
impl<C: Connect> Forward for C {
    type ForwardTo = Passthrough<C::Connection>;

    async fn forward_connect(
        &self,
        address: &TcpAddress,
    ) -> Result<Self::ForwardTo, std::io::Error> {
        Ok(Passthrough::new(self.connect(address).await?))
    }
}

pub struct Passthrough<T> {
    target: T,
}

impl<T> Passthrough<T> {
    pub fn new(target: T) -> Self {
        Self { target }
    }

    pub fn into_target(self) -> T {
        self.target
    }
}

#[async_trait]
impl<T: AsyncRead + AsyncWrite + Send + Sync + Unpin> ForwardTo for Passthrough<T> {
    async fn forward<S: AsyncRead + AsyncWrite + Send + Sync + Unpin>(
        mut self,
        mut source: S,
    ) -> Result<(), std::io::Error> {
        tokio::io::copy_bidirectional(&mut source, &mut self.target).await?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct LayeredForward<C, L> {
    connect: C,
    layer: L,
}

impl<C, L> LayeredForward<C, L> {
    pub fn new(connect: C, layer: L) -> Self {
        Self {
            connect,
            layer,
        }
    }
}

#[async_trait]
impl<C: Connect, L: Clone> Forward for LayeredForward<C, L> {
    type ForwardTo = LayeredForwardTo<C::Connection, L>;

    async fn forward_connect(
        &self,
        address: &TcpAddress,
    ) -> Result<Self::ForwardTo, std::io::Error> {
        Ok(LayeredForwardTo::new(self.connect(address).await?, self.layer.clone()))
    }
}

#[derive(Clone, Debug)]
pub struct LayeredForwardTo<T, L> {
    target: T,
    layer: L,
}

impl<T, L> LayeredForwardTo<T, L> {
    pub fn new(target: T, layer: L) -> Self {
        Self {
            target,
            layer,
        }
    }
}

#[async_trait]
impl<T, L> ForwardTo for LayeredForward<T, L>
where
    T: AsyncRead + AsyncWrite + Send + Sync + Unpin,
    L: Layer<Pin<Box<dyn AsyncRead + AsyncWrite + Send + Sync + Unpin>>, T>,
{
    async fn forward<S: AsyncRead + AsyncWrite + Send + Sync + Unpin>(
        self,
        source: S,
    ) -> Result<(), std::io::Error> {
        self.layer.layer(source, self.target).await?;
        Ok(())
    }
}
