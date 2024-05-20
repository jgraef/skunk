pub mod body;

use std::{
    convert::Infallible,
    pin::Pin,
    sync::Arc,
    task::{
        Context,
        Poll,
    },
};

use bytes::Bytes;
use futures::{
    Future,
    TryFutureExt,
};
use http_body_util::BodyExt;
use hyper::{
    body::{
        Body,
        Incoming,
    },
    service::service_fn,
    StatusCode,
};
pub use hyper::{
    Request,
    Response,
};
use hyper_util::rt::TokioIo;
use tokio::{
    io::{
        AsyncRead,
        AsyncWrite,
    },
    sync::{
        mpsc,
        oneshot,
        Mutex,
    },
};

use self::body::Empty;
use crate::util::{
    Rewind,
    WithoutShutdown,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("hyper error")]
    Hyper(#[from] hyper::Error),

    #[error("connection closed")]
    ConnectionClosed,
}

pub async fn server<T, H>(io: T, request_handler: H) -> Result<(), crate::Error>
where
    T: AsyncRead + AsyncWrite + Unpin,
    H: RequestHandler,
    Bytes: From<<H::ResponseBody as Body>::Data>,
    <H::ResponseBody as Body>::Error: std::error::Error + Send + Sync + 'static,
{
    let (tx_req, mut rx_req) = mpsc::channel(16);

    let conn = hyper::server::conn::http1::Builder::new()
        .serve_connection(
            TokioIo::new(WithoutShutdown::new(io)),
            service_fn(move |request: Request<Incoming>| {
                let tx_req = tx_req.clone();
                async move {
                    let (tx_resp, rx_resp) = oneshot::channel::<Response<H::ResponseBody>>();

                    // todo: can this be dropped before source_conn is dropped, and what do we
                    // do then?
                    tx_req
                        .send((request, tx_resp))
                        .await
                        .expect("bug: all receivers for server request have been dropped");

                    // receive the response from the layer future.
                    let response = rx_resp
                        .await
                        .map(|response| {
                            response.map(|body| {
                                http_body_util::Either::Left(
                                    body.map_frame(|frame| frame.map_data(Into::into)),
                                )
                            })
                        })
                        .unwrap_or_else(|_| {
                            // when the inner layer fails, we won't get a response, so we'll
                            // instead return 502
                            tracing::debug!("the server response sender has been dropped");
                            Response::builder()
                                .status(StatusCode::BAD_GATEWAY)
                                .body(http_body_util::Either::Right(Empty))
                                .expect("constructed invalid http response")
                        });

                    Ok::<_, Infallible>(response)
                }
            }),
        )
        .without_shutdown()
        .map_err(|e| Error::Hyper(e)) // why does `Error::from` not work here?
        .map_err(crate::Error::from);

    let handler_fut = async move {
        while let Some((request, tx_resp)) = rx_req.recv().await {
            let response = request_handler.handle_request(request).await?;

            tx_resp
                .send(response)
                .unwrap_or_else(|_| panic!("bug: the server response receiver has been dropped"));
        }
        Ok::<(), crate::Error>(())
    };

    let (conn_result, handler_result) = tokio::join!(conn, handler_fut);
    handler_result?;
    let _io = {
        let parts = conn_result?;
        let without_shutdown = parts.io.into_inner();
        assert!(
            !without_shutdown.was_shutdown(),
            "fixme: underlying IO was shutdown"
        );
        let io = without_shutdown.into_inner();
        Rewind::new(io, parts.read_buf)
    };

    // todo: perform protocol upgrade

    Ok(())
}

pub trait RequestHandler<RequestBody = Incoming> {
    type ResponseBody: Body + 'static;

    fn handle_request(
        &self,
        request: Request<RequestBody>,
    ) -> impl Future<Output = Result<Response<Self::ResponseBody>, crate::Error>> + Send
    where
        Bytes: From<<Self::ResponseBody as Body>::Data>,
        <Self::ResponseBody as Body>::Error: std::error::Error + Send + 'static;
}

pub fn fn_handler<F, Fut, RequestBody, ResponseBody>(func: F) -> FnHandler<F>
where
    F: Fn(Request<RequestBody>) -> Fut,
    Fut: Future<Output = Result<Response<ResponseBody>, crate::Error>> + Send,
    ResponseBody: Body + 'static,
{
    FnHandler { func }
}

#[derive(Clone, Copy)]
pub struct FnHandler<F> {
    func: F,
}

impl<F, Fut, RequestBody, ResponseBody> RequestHandler<RequestBody> for FnHandler<F>
where
    F: Fn(Request<RequestBody>) -> Fut,
    Fut: Future<Output = Result<Response<ResponseBody>, crate::Error>> + Send,
    ResponseBody: Body + 'static,
{
    type ResponseBody = ResponseBody;

    fn handle_request(
        &self,
        request: Request<RequestBody>,
    ) -> impl Future<Output = Result<Response<ResponseBody>, crate::Error>> + Send
    where
        Bytes: From<<ResponseBody as Body>::Data>,
        <Self::ResponseBody as Body>::Error: std::error::Error + Send + 'static,
    {
        (self.func)(request)
    }
}

#[derive(Debug)]
pub struct Client<T, B>
where
    T: AsyncRead + AsyncWrite,
    B: Body + 'static,
{
    connection: Option<hyper::client::conn::http1::Connection<TokioIo<WithoutShutdown<T>>, B>>,
}

impl<T, B> Future for Client<T, B>
where
    T: AsyncRead + AsyncWrite + Unpin,
    B: Body + 'static,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    type Output = Result<Rewind<T>, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(connection) = &mut self.connection {
            match connection.poll_without_shutdown(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
                Poll::Ready(Ok(())) => {
                    let parts = self.connection.take().unwrap().into_parts();
                    let without_shutdown = parts.io.into_inner();
                    assert!(
                        !without_shutdown.was_shutdown(),
                        "fixme: underlying IO was shutdown"
                    );
                    let io = without_shutdown.into_inner();
                    let io = Rewind::new(io, parts.read_buf);
                    Poll::Ready(Ok(io))
                }
            }
        }
        else {
            Poll::Ready(Err(Error::ConnectionClosed))
        }
    }
}

#[derive(Debug)]
pub struct SendRequest<RequestBody>
where
    RequestBody: Body + 'static,
{
    send_request: Arc<Mutex<hyper::client::conn::http1::SendRequest<RequestBody>>>,
}

impl<RequestBody> Clone for SendRequest<RequestBody>
where
    RequestBody: Body + 'static,
{
    fn clone(&self) -> Self {
        Self {
            send_request: self.send_request.clone(),
        }
    }
}

impl<RequestBody> SendRequest<RequestBody>
where
    RequestBody: Body + Send + 'static,
    RequestBody::Error: std::error::Error + Send + Sync + 'static,
{
    pub async fn send(&self, request: Request<RequestBody>) -> Result<Response<Incoming>, Error> {
        let mut send_request = self.send_request.lock().await;
        Ok(send_request.send_request(request).await?)
    }
}

impl<RequestBody> RequestHandler<RequestBody> for SendRequest<RequestBody>
where
    RequestBody: Body + Send + 'static,
    RequestBody::Error: std::error::Error + Send + Sync + 'static,
{
    type ResponseBody = Incoming;

    async fn handle_request(
        &self,
        request: Request<RequestBody>,
    ) -> Result<Response<Self::ResponseBody>, crate::Error>
    where
        Bytes: From<<Self::ResponseBody as Body>::Data>,
        Error: From<<Self::ResponseBody as Body>::Error>,
        <Self::ResponseBody as Body>::Error: std::error::Error + Send + 'static,
    {
        Ok(self.send(request).await?)
    }
}

pub async fn client<T, B>(io: T) -> Result<(Client<T, B>, SendRequest<B>), Error>
where
    T: AsyncRead + AsyncWrite + Unpin,
    B: Body + 'static,
    B::Data: Send,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let (send_request, connection) = hyper::client::conn::http1::Builder::new()
        .handshake(TokioIo::new(WithoutShutdown::new(io)))
        .await?;

    let client = Client {
        connection: Some(connection),
    };

    let send_request = SendRequest {
        send_request: Arc::new(Mutex::new(send_request)),
    };

    Ok((client, send_request))
}

pub async fn proxy<I, O, F, Fut, Bq, Bs>(incoming: I, outgoing: O, f: F) -> Result<(), crate::Error>
where
    I: AsyncRead + AsyncWrite + Unpin,
    O: AsyncRead + AsyncWrite + Unpin,
    F: Fn(Request<Incoming>, SendRequest<Bq>) -> Fut,
    Fut: Future<Output = Result<Response<Bs>, crate::Error>> + Send,
    Bq: Body + 'static,
    Bq::Data: Send,
    Bq::Error: std::error::Error + Send + Sync + 'static,
    Bs: Body + 'static,
    Bytes: From<Bs::Data>,
    Bs::Error: std::error::Error + Send + Sync + 'static,
{
    let (client, send_request) = client(outgoing).await?;

    tokio::try_join!(
        server(
            incoming,
            fn_handler(|request| {
                let send_request = send_request.clone();
                f(request, send_request)
            })
        ),
        client.map_err(crate::Error::from),
    )?;

    Ok(())
}
