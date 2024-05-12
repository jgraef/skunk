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

use crate::layer::Layer;

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
    L: for<'client> Layer<Request, TargetClient<'client>, Output = Response> + Sync,
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    T: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Output = ();

    async fn layer(&self, source: S, target: T) -> Result<(), crate::Error> {
        // create client connection to target
        let (send_request, target_conn) = hyper::client::conn::http1::Builder::new()
            .handshake(TokioIo::new(target))
            .await
            .map_err(Error::from)?;
        let send_request = Arc::new(Mutex::new(send_request));

        // create server connection to source
        let inner = &self.inner;
        let source_conn = hyper::server::conn::http1::Builder::new().serve_connection(
            TokioIo::new(source),
            service_fn(move |request: hyper::Request<Incoming>| {
                let send_request = send_request.clone();
                async move {
                    // todo: what to do with the error? should we bubble it up through the layers?
                    let mut send_request = send_request.lock().await;
                    let response = inner
                        .layer(
                            Request(request),
                            TargetClient {
                                send_request: &mut send_request,
                            },
                        )
                        .await?;
                    Ok::<_, crate::Error>(response.0)
                }
            }),
        );

        // await both connections
        tokio::try_join!(target_conn, source_conn).map_err(Error::from)?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct Request(pub hyper::Request<Incoming>);

#[derive(Debug)]
pub struct Response(pub hyper::Response<Incoming>);

#[derive(Debug)]
pub struct TargetClient<'client> {
    send_request: &'client mut SendRequest<Incoming>,
}

impl<'client> TargetClient<'client> {
    pub async fn send(&mut self, request: Request) -> Result<Response, Error> {
        let response = self.send_request.send_request(request.0).await?;
        Ok(Response(response))
    }
}
