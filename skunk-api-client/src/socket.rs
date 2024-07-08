use std::fmt::Debug;

use futures_util::{
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
    socket::{
        ClientHello,
        ClientMessage,
        ServerHello,
        ServerMessage,
        Version,
    },
    PROTOCOL_VERSION,
};
use skunk_util::trigger;
use tokio::sync::{
    mpsc,
    watch,
};
use url::Url;

use crate::Status;

pub const USER_AGENT: &'static str = std::env!("CARGO_PKG_NAME");
lazy_static! {
    pub static ref CLIENT_VERSION: Version = std::env!("CARGO_PKG_VERSION").parse().unwrap();
}

#[derive(Debug, thiserror::Error)]
#[error("Reactor error")]
enum Error {
    #[error("Websocket disconnected")]
    Disconnected,
    #[error("Handshake failed")]
    Handshake,
    Reqwest(#[from] reqwest::Error),
    Websocket(#[from] reqwest_websocket::Error),
    Decode(#[from] rmp_serde::decode::Error),
    Encode(#[from] rmp_serde::encode::Error),
}

#[derive(Clone, Debug)]
pub(crate) struct ReactorHandle {
    pub command_tx: mpsc::Sender<Command>,
    pub reload_rx: trigger::Receiver,
    pub status_rx: watch::Receiver<Status>,
    pub pong_rx: trigger::Receiver,
}

/// Reactor that handles the websocket connection to the server.
///
/// The client can send commands through the sender half of `command_rx`.
pub(crate) struct Reactor {
    client: reqwest::Client,
    url: Url,
    command_rx: mpsc::Receiver<Command>,
    reload_tx: trigger::Sender,
    status_tx: watch::Sender<Status>,
    pong_tx: trigger::Sender,
    flows_tx: Option<mpsc::Sender<()>>,
}

impl Reactor {
    pub fn new(client: reqwest::Client, url: Url) -> (Self, ReactorHandle) {
        let (command_tx, command_rx) = mpsc::channel(16);
        let (reload_tx, reload_rx) = trigger::new();
        let (status_tx, status_rx) = watch::channel(Default::default());
        let (pong_tx, pong_rx) = trigger::new();

        let this = Self {
            client,
            url,
            command_rx,
            reload_tx,
            status_tx,
            pong_tx,
            flows_tx: None,
        };

        let handle = ReactorHandle {
            command_tx,
            reload_rx,
            status_rx,
            pong_rx,
        };

        (this, handle)
    }

    pub async fn run(mut self) {
        loop {
            let Ok(connection) = ReactorConnection::connect(&mut self).await
            else {
                // connection failed
                // we should retry after some time, but since we don't have a reliable sleep,
                // we'll panic for now
                todo!();
            };

            match connection.run().await {
                Ok(()) => {
                    // ReactorConnection returns Ok(()) when the command sender has been dropped, so
                    // we should terminate
                    let _ = self.status_tx.send(Status::Disconnected);
                    break;
                }
                Err(Error::Disconnected) => {
                    // the websocket connection was disconnected for some reason. so, we'll try to
                    // reconnect todo: wait for some time
                    let _ = self.status_tx.send(Status::Disconnected);
                }
                Err(e) => {
                    tracing::error!("Reactor failed: {e}");
                    break;
                }
            }
        }
    }
}

struct ReactorConnection<'a> {
    socket: WebSocket,
    reactor: &'a mut Reactor,
}

impl<'a> ReactorConnection<'a> {
    async fn connect(reactor: &'a mut Reactor) -> Result<Self, Error> {
        let mut socket: WebSocket = reactor
            .client
            .get(reactor.url.clone())
            .upgrade()
            .send()
            .await?
            .into_websocket()
            .await?
            .into();

        socket
            .send(&ClientHello {
                user_agent: USER_AGENT.into(),
                app_version: CLIENT_VERSION.clone(),
                protocol_version: PROTOCOL_VERSION,
            })
            .await?;

        let _server_hello: ServerHello = socket.receive().await?.ok_or_else(|| Error::Handshake)?;

        let _ = reactor.status_tx.send(Status::Connected);

        Ok(Self { socket, reactor })
    }

    async fn run(mut self) -> Result<(), Error> {
        loop {
            tokio::select! {
                message_res = self.socket.receive() => {
                    let Some(message) = message_res? else {
                        tracing::debug!("Connection closed");
                        break Err(Error::Disconnected);
                    };
                    self.handle_message(message).await?;
                }
                command_opt = self.reactor.command_rx.recv() => {
                    let Some(command) = command_opt else {
                        tracing::debug!("Command sender dropped");
                        break Ok(());
                    };
                    self.handle_command(command).await?;
                }
            }
        }
    }

    async fn handle_message(&mut self, message: ServerMessage) -> Result<(), Error> {
        tracing::debug!(?message, "received");

        match message {
            ServerMessage::HotReload => {
                self.reactor.reload_tx.trigger();
            }
            ServerMessage::Pong => {
                self.reactor.pong_tx.trigger();
            }
            ServerMessage::Interrupt { message_id } => {
                // todo: for now we'll just send a Continue back
                // eventually we want to send the interrupt to the user with a oneshot channel.
                self.socket
                    .send(&ClientMessage::Continue { message_id })
                    .await?;
            }
            ServerMessage::Flow { .. } => {
                if let Some(flows_tx) = &mut self.reactor.flows_tx {
                    if let Err(_) = flows_tx.send(()).await {
                        // the flows receiver has been dropped.
                        self.reactor.flows_tx = None;
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_command(&mut self, command: Command) -> Result<(), Error> {
        match command {
            Command::Ping => {
                self.socket.send(&ClientMessage::Ping).await?;
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub(crate) enum Command {
    Ping,
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
