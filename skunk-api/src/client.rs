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
use reqwest_websocket::{
    Message,
    RequestBuilderExt,
};
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::{
    mpsc,
    watch,
};
use tracing::Instrument;
use url::Url;

use crate::{
    protocol::{
        ClientMessage,
        ServerMessage,
    },
    util::Ids,
};

#[derive(Debug, thiserror::Error)]
#[error("API client error")]
pub enum Error {
    Reqwest(#[from] reqwest::Error),
    Websocket(#[from] reqwest_websocket::Error),
    Decode(#[from] rmp_serde::decode::Error),
    Encode(#[from] rmp_serde::encode::Error),
}

#[derive(Clone, Debug)]
pub struct Client {
    client: reqwest::Client,
    base_url: UrlBuilder,
    command_tx: mpsc::Sender<Command>,
    reload_rx: watch::Receiver<()>,
}

impl Client {
    pub fn new(base_url: Url) -> (Self, Connection) {
        let client = reqwest::Client::new();
        let base_url = UrlBuilder { url: base_url };
        let (command_tx, command_rx) = mpsc::channel(4);
        let (reload_tx, reload_rx) = watch::channel(());

        let connection = Connection {
            inner: Box::pin({
                let client = client.clone();
                let base_url = base_url.clone();
                let span = tracing::info_span!("connection");
                async move {
                    let reactor = Reactor::new(client, base_url, command_rx, reload_tx).await?;
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
        };

        (client, connection)
    }

    pub fn hot_reload(&self) -> HotReload {
        HotReload {
            reload_rx: self.reload_rx.clone(),
        }
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
    websocket: WebSocket,
    command_rx: mpsc::Receiver<Command>,
    reload_tx: watch::Sender<()>,
    ids: Ids,
    flows_tx: Option<mpsc::Sender<()>>,
}

impl Reactor {
    async fn new(
        client: reqwest::Client,
        base_url: UrlBuilder,
        command_rx: mpsc::Receiver<Command>,
        reload_tx: watch::Sender<()>,
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
            websocket,
            command_rx,
            reload_tx,
            ids: Ids::new(Ids::SCOPE_CLIENT),
            flows_tx: None,
        })
    }

    async fn run(mut self) -> Result<(), Error> {
        loop {
            tokio::select! {
                message_res = self.websocket.receive() => {
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
                self.websocket
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
