use futures::Future;
use tokio::io::{
    AsyncRead,
    AsyncWrite,
};

use crate::address::TcpAddress;

//#[cfg(feature = "http")]
//pub mod http;
#[cfg(feature = "socks")]
pub mod socks;

pub trait TargetAddress {
    fn target_address(&self) -> &TcpAddress;
}

pub trait Proxy<I, O> {
    type Error: std::error::Error;

    fn proxy(
        &self,
        incoming: I,
        outgoing: O,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

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

pub fn fn_proxy<F, E, Fut, I, O>(func: F) -> FnProxy<F>
where
    F: Fn(I, O) -> Fut,
    Fut: Future<Output = Result<(), E>>,
{
    FnProxy { func }
}

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
