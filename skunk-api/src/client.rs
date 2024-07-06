#![allow(dead_code)]

use futures_util::{
    SinkExt,
    TryFutureExt,
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
use tokio::sync::mpsc;
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

#[derive(Debug)]
pub struct Client {
    client: reqwest::Client,
    base_url: UrlBuilder,
    command_tx: mpsc::Sender<Command>,
}

impl Client {
    pub fn new(base_url: Url) -> Self {
        let client = reqwest::Client::new();
        let base_url = UrlBuilder { url: base_url };
        let (command_tx, command_rx) = mpsc::channel(4);

        Reactor::spawn(client.clone(), base_url.clone(), command_rx);

        Self {
            client,
            base_url,
            command_tx,
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

/// Reactor that handles the websocket connection to the server.
///
/// The client can send commands through the sender half of `command_rx`.
struct Reactor {
    websocket: WebSocket,
    command_rx: mpsc::Receiver<Command>,
    ids: Ids,
    flows_tx: Option<mpsc::Sender<()>>,
}

impl Reactor {
    fn spawn(client: reqwest::Client, base_url: UrlBuilder, command_rx: mpsc::Receiver<Command>) {
        let span = tracing::info_span!("client");

        // needs to be local, because the WebSocket is not `Send + Sync` on WASM.
        tokio::task::spawn_local(
            async move {
                Reactor::new(client, base_url, command_rx)
                    .await?
                    .run()
                    .await?;
                Ok::<(), Error>(())
            }
            .instrument(span)
            .map_err(|e| {
                tracing::error!("{e}");
            }),
        );
    }

    async fn new(
        client: reqwest::Client,
        base_url: UrlBuilder,
        command_rx: mpsc::Receiver<Command>,
    ) -> Result<Self, Error> {
        let websocket = WebSocket::new(
            client
                .get(base_url.push("ws").finish())
                .upgrade()
                .send()
                .await?
                .into_websocket()
                .await?,
        );

        Ok(Self {
            websocket,
            command_rx,
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
        match message {
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

impl WebSocket {
    pub fn new(inner: reqwest_websocket::WebSocket) -> Self {
        Self { inner }
    }

    pub async fn receive<T: for<'de> Deserialize<'de>>(&mut self) -> Result<Option<T>, Error> {
        while let Some(message) = self.inner.try_next().await? {
            match message {
                Message::Binary(data) => {
                    let item: T = rmp_serde::from_slice(&data)?;
                    return Ok(Some(item));
                }
                _ => {}
            }
        }

        Ok(None)
    }

    pub async fn send<T: Serialize>(&mut self, item: &T) -> Result<(), Error> {
        let data = rmp_serde::to_vec(item)?;
        self.inner.send(Message::Binary(data)).await?;
        Ok(())
    }
}
