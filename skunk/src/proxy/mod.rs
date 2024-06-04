//! Proxy implementations.

//#[cfg(feature = "http")]
//pub mod http;
#[cfg(feature = "socks")]
pub mod socks;

use futures::Future;
use tokio::io::{
    AsyncRead,
    AsyncWrite,
};

use crate::address::TcpAddress;

/// Trait for connections that have an associated destination address.
pub trait DestinationAddress {
    fn destination_address(&self) -> &TcpAddress;
}

/// Trait for things that can proxy (i.e. forward) connections.
pub trait Proxy<I, O> {
    type Error: std::error::Error;

    /// Proxy/forward the `incoming` connection to the `outgoing` connection,
    /// and vice-versa.
    fn proxy(
        &self,
        incoming: I,
        outgoing: O,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// A [`Proxy`] implementation that just copies the data bidirectionally.
#[derive(Clone, Copy, Debug, Default)]
pub struct Passthrough;

impl<I, O> Proxy<I, O> for Passthrough
where
    I: AsyncRead + AsyncWrite + Send,
    O: AsyncRead + AsyncWrite + Send,
{
    type Error = std::io::Error;

    async fn proxy(&self, incoming: I, outgoing: O) -> Result<(), Self::Error> {
        tokio::pin!(incoming);
        tokio::pin!(outgoing);
        tokio::io::copy_bidirectional(&mut incoming, &mut outgoing).await?;
        Ok(())
    }
}

/// Create a [`Proxy`] using a function or closure.
pub fn fn_proxy<F, E, Fut, I, O>(func: F) -> FnProxy<F>
where
    F: Fn(I, O) -> Fut,
    Fut: Future<Output = Result<(), E>>,
{
    FnProxy { func }
}

/// A [`Proxy`] created from a function or closure.
#[derive(Copy, Clone)]
pub struct FnProxy<F> {
    func: F,
}

impl<F, E, Fut, I, O> Proxy<I, O> for FnProxy<F>
where
    F: Fn(I, O) -> Fut,
    Fut: Future<Output = Result<(), E>> + Send,
    E: std::error::Error,
{
    type Error = E;

    fn proxy(
        &self,
        incoming: I,
        outgoing: O,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        (self.func)(incoming, outgoing)
    }
}
