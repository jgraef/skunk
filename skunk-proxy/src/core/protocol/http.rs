use std::sync::Arc;

use hyper::{
    body::Incoming,
    client::conn::http1::SendRequest,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use tokio::{
    io::{
        AsyncRead,
        AsyncWrite,
    },
    sync::Mutex,
};

use crate::core::layer::Layer;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("hyper error")]
    Hyper(#[from] hyper::Error),
}

#[derive(Clone, Debug)]
pub struct Http<L> {
    inner: L,
}

impl<L, S, T> Layer<S, T> for Http<L>
where
    L: Layer<Request, Client, Output = Response> + Sync,
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    T: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Output = ();

    async fn layer(&self, source: S, target: T) -> Result<(), crate::core::Error> {
        // create client connection to target
        let (send_request, target_conn) = hyper::client::conn::http1::Builder::new()
            .handshake(TokioIo::new(target))
            .await
            .map_err(Error::from)?;
        let client = Client {
            send_request: Arc::new(Mutex::new(send_request)),
        };

        // create server connection to source
        let inner = &self.inner;
        let source_conn = hyper::server::conn::http1::Builder::new().serve_connection(
            TokioIo::new(source),
            service_fn(move |request: hyper::Request<Incoming>| {
                let client = client.clone();
                async move {
                    // todo: what to do with the error? should we bubble it up through the layers?
                    let response = inner.layer(Request { inner: request }, client).await?;
                    Ok::<_, crate::core::Error>(response.inner)
                }
            }),
        );

        // await both connections
        tokio::try_join!(target_conn, source_conn).map_err(Error::from)?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct Request {
    pub inner: hyper::Request<Incoming>,
}

#[derive(Debug)]
pub struct Response {
    pub inner: hyper::Response<Incoming>,
}

#[derive(Clone, Debug)]
pub struct Client {
    send_request: Arc<Mutex<SendRequest<Incoming>>>,
}
