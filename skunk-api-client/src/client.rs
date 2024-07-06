#![allow(dead_code)]

use std::{
    fmt::Debug,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use futures_util::{
    Future,
    FutureExt,
    SinkExt,
    TryStreamExt,
};
use lazy_static::lazy_static;
use reqwest_websocket::{
    Message,
    RequestBuilderExt,
};
use serde::{
    Deserialize,
    Serialize,
};
use skunk_api_protocol::{
    protocol::{
        ClientHello,
        ClientMessage,
        ServerHello,
        ServerMessage,
        Version,
    },
    util::Ids,
    PROTOCOL_VERSION,
};
use tokio::sync::{
    mpsc,
    watch,
};
use tracing::Instrument;
use url::Url;

use super::Error;

pub const USER_AGENT: &'static str = std::env!("CARGO_PKG_NAME");
lazy_static! {
    pub static ref CLIENT_VERSION: Version = std::env!("CARGO_PKG_VERSION").parse().unwrap();
}

#[derive(Clone, Debug)]
pub struct Client {
    client: reqwest::Client,
    base_url: UrlBuilder,
    command_tx: mpsc::Sender<Command>,
    reload_rx: watch::Receiver<()>,
    status_rx: watch::Receiver<Status>,
}

impl Client {
    pub fn new(base_url: Url) -> (Self, Connection) {
        let client = reqwest::Client::new();
        let base_url = UrlBuilder { url: base_url };
        let (command_tx, command_rx) = mpsc::channel(4);
        let (reload_tx, reload_rx) = watch::channel(());
        let (status_tx, status_rx) = watch::channel(Default::default());

        let connection = Connection {
            inner: Box::pin({
                let client = client.clone();
                let base_url = base_url.clone();
                let span = tracing::info_span!("connection");
                async move {
                    let reactor =
                        Reactor::new(client, base_url, command_rx, reload_tx, status_tx).await?;
                    reactor.run().await
                }
                .instrument(span)
            }),
        };

        let client = Self {
            client,
            base_url,
            command_tx,
            reload_rx,
            status_rx,
        };

        (client, connection)
    }

    pub fn hot_reload(&self) -> HotReload {
        HotReload {
            reload_rx: self.reload_rx.clone(),
        }
    }

    pub fn status(&self) -> watch::Receiver<Status> {
        self.status_rx.clone()
    }
}

#[derive(Clone, Debug)]
struct UrlBuilder {
    url: Url,
}

impl UrlBuilder {
    pub fn push(mut self, s: impl ToString) -> Self {
        self.url.path_segments_mut().unwrap().push(&s.to_string());
        self
    }

    pub fn finish(self) -> Url {
        self.url
    }
}

/// Client connection.
///
/// This must be polled to drive the connection for a [`Client`].
pub struct Connection {
    inner: Pin<Box<dyn Future<Output = Result<(), Error>>>>,
}

impl Future for Connection {
    type Output = Result<(), Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.inner.poll_unpin(cx)
    }
}

impl Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection").finish_non_exhaustive()
    }
}

/// Reactor that handles the websocket connection to the server.
///
/// The client can send commands through the sender half of `command_rx`.
struct Reactor {
    socket: WebSocket,
    command_rx: mpsc::Receiver<Command>,
    reload_tx: watch::Sender<()>,
    status_tx: watch::Sender<Status>,
    ids: Ids,
    flows_tx: Option<mpsc::Sender<()>>,
}

impl Reactor {
    async fn new(
        client: reqwest::Client,
        base_url: UrlBuilder,
        command_rx: mpsc::Receiver<Command>,
        reload_tx: watch::Sender<()>,
        status_tx: watch::Sender<Status>,
    ) -> Result<Self, Error> {
        let websocket = client
            .get(base_url.push("ws").finish())
            .upgrade()
            .send()
            .await?
            .into_websocket()
            .await?
            .into();

        Ok(Self {
            socket: websocket,
            command_rx,
            reload_tx,
            status_tx,
            ids: Ids::new(Ids::SCOPE_CLIENT),
            flows_tx: None,
        })
    }

    async fn run(mut self) -> Result<(), Error> {
        self.socket
            .send(&ClientHello {
                user_agent: USER_AGENT.into(),
                app_version: CLIENT_VERSION.clone(),
                protocol_version: PROTOCOL_VERSION,
            })
            .await?;

        let _server_hello: ServerHello = self
            .socket
            .receive()
            .await?
            .ok_or_else(|| Error::Protocol)?;

        let _ = self.status_tx.send(Status::Connected);

        loop {
            tokio::select! {
                message_res = self.socket.receive() => {
                    let Some(message) = message_res? else {
                        tracing::debug!("Connection closed");
                        break;
                    };
                    self.handle_message(message).await?;
                }
                command_opt = self.command_rx.recv() => {
                    let Some(command) = command_opt else {
                        tracing::debug!("Command sender dropped");
                        break;
                    };
                    self.handle_command(command).await?;
                }
            }
        }

        let _ = self.status_tx.send(Status::Disconnected);

        Ok(())
    }

    async fn handle_message(&mut self, message: ServerMessage) -> Result<(), Error> {
        tracing::debug!(?message, "received");

        match message {
            ServerMessage::HotReload => {
                let _ = self.reload_tx.send(());
            }
            ServerMessage::Interrupt { continue_tx } => {
                // todo: for now we'll just send a Continue back
                // eventually we want to send the interrupt to the user with a oneshot channel.
                self.socket
                    .send(&ClientMessage::Continue { continue_tx })
                    .await?;
            }
            ServerMessage::Flow { .. } => {
                if let Some(flows_tx) = &mut self.flows_tx {
                    if let Err(_) = flows_tx.send(()).await {
                        // the flows receiver has been dropped.
                        self.flows_tx = None;
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_command(&mut self, _command: Command) -> Result<(), Error> {
        todo!();
    }
}

enum Command {
    // todo
}

/// Wrapper around [`reqwest_websocket::WebSocket`] that sends and receives
/// msgpack-encoded messages.
#[derive(Debug)]
struct WebSocket {
    inner: reqwest_websocket::WebSocket,
}

impl From<reqwest_websocket::WebSocket> for WebSocket {
    fn from(inner: reqwest_websocket::WebSocket) -> Self {
        Self { inner }
    }
}

impl WebSocket {
    async fn receive<T: for<'de> Deserialize<'de>>(&mut self) -> Result<Option<T>, Error> {
        while let Some(message) = self.inner.try_next().await? {
            match message {
                Message::Binary(data) => {
                    let item: T = rmp_serde::from_slice(&data)?;
                    return Ok(Some(item));
                }
                Message::Close { .. } => return Ok(None),
                _ => {}
            }
        }

        Ok(None)
    }

    async fn send<T: Serialize>(&mut self, item: &T) -> Result<(), Error> {
        let data = rmp_serde::to_vec(item)?;
        self.inner.send(Message::Binary(data)).await?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct HotReload {
    reload_rx: watch::Receiver<()>,
}

impl HotReload {
    pub async fn wait(&mut self) {
        if self.reload_rx.changed().await.is_err() {
            tracing::debug!("hot_reload sender dropped");
            futures_util::future::pending::<()>().await;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Disconnected,
    Connected,
}
