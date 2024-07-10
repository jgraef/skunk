use std::{
    collections::HashMap,
    fmt::Debug,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
    time::Duration,
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
use semver::Version;
use semver_macro::env_version;
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
        SubscriptionId,
    },
    PROTOCOL_VERSION,
};
use skunk_util::trigger;
use tokio::sync::{
    mpsc,
    watch,
};
use url::Url;

use crate::{
    flow,
    util::platform::{
        interval,
        sleep,
        Sleep,
    },
    Status,
};

pub const USER_AGENT: &'static str = std::env!("CARGO_PKG_NAME");
pub const CLIENT_VERSION: Version = env_version!("CARGO_PKG_VERSION");

#[derive(Debug, thiserror::Error)]
#[error("Reactor error")]
enum Error {
    #[error("Websocket disconnected")]
    Disconnected,
    #[error("Handshake failed")]
    Handshake,
    #[error("Ping timed out")]
    PingTimeout,
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
    flows_tx: HashMap<SubscriptionId, mpsc::Sender<flow::Event>>,
}

impl Reactor {
    pub fn new(client: reqwest::Client, url: Url) -> (Self, ReactorHandle) {
        let (command_tx, command_rx) = mpsc::channel(16);
        let (reload_tx, reload_rx) = trigger::new();
        let (status_tx, status_rx) = watch::channel(Default::default());

        let this = Self {
            client,
            url,
            command_rx,
            reload_tx,
            status_tx,
            flows_tx: HashMap::new(),
        };

        let handle = ReactorHandle {
            command_tx,
            reload_rx,
            status_rx,
        };

        (this, handle)
    }

    pub async fn run(mut self) {
        loop {
            let Ok(connection) = ReactorConnection::connect(&mut self).await
            else {
                // connection failed
                tracing::warn!("Connection failed: {}", self.url);
                sleep(Duration::from_secs(5)).await;
                continue;
            };

            match connection.run().await {
                Ok(()) => {
                    // ReactorConnection returns Ok(()) when the command sender has been dropped, so
                    // we should terminate
                    tracing::info!("Shutting down");
                    let _ = self.status_tx.send(Status::Disconnected);
                    break;
                }
                Err(Error::Disconnected) | Err(Error::PingTimeout) => {
                    // the websocket connection was disconnected for some reason. so, we'll try to
                    // reconnect
                    //
                    // todo: should we wait here?
                    tracing::info!("Disconnected");
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
    ping_timeout: PingTimeout,
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

        Ok(Self {
            socket,
            reactor,
            ping_timeout: PingTimeout::new(Duration::from_secs(5)),
        })
    }

    async fn run(mut self) -> Result<(), Error> {
        let mut ping_interval = interval(Duration::from_secs(10));

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
                _ = ping_interval.tick() => {
                    self.socket.send(&ClientMessage::Ping).await?;
                    self.ping_timeout.start();
                }
                _ = &mut self.ping_timeout => {
                    break Err(Error::PingTimeout);
                }
            }
        }
    }

    async fn handle_message(&mut self, message: ServerMessage) -> Result<(), Error> {
        tracing::debug!(?message, "received");

        match message {
            ServerMessage::ReloadUi => {
                self.reactor.reload_tx.trigger();
            }
            ServerMessage::Pong => {
                self.ping_timeout.clear();
            }
            ServerMessage::FlowEvent {
                subscription_id,
                event,
            } => {
                // todo: Our implementation only supports one subscription at a time right now.

                if let Some(flows_tx) = self.reactor.flows_tx.get_mut(&subscription_id) {
                    if let Err(_) = flows_tx.send(event).await {
                        // the flows receiver has been dropped.
                        self.reactor.flows_tx.remove(&subscription_id);
                    }
                }
                else {
                    // we don't have anyone listening to that subscription ID, so we can tell the
                    // server to unsubscribe us.
                    self.socket
                        .send(&ClientMessage::Unsubscribe { subscription_id })
                        .await?;
                }
            }
            ServerMessage::Interrupt { message_id } => {
                // todo: for now we'll just send a Continue back
                // eventually we want to send the interrupt to the user with a oneshot channel.
                self.socket
                    .send(&ClientMessage::Continue { message_id })
                    .await?;
            }
        }

        Ok(())
    }

    async fn handle_command(&mut self, command: Command) -> Result<(), Error> {
        match command {
            Command::SubscribeFlowEvents {
                subscription_id,
                event_tx,
            } => {
                self.reactor.flows_tx.insert(subscription_id, event_tx);
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub(crate) enum Command {
    SubscribeFlowEvents {
        subscription_id: SubscriptionId,
        event_tx: mpsc::Sender<flow::Event>,
    },
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

#[derive(Debug)]
struct PingTimeout {
    sleep: Option<Sleep>,
    timeout: Duration,
}

impl PingTimeout {
    pub fn new(timeout: Duration) -> Self {
        Self {
            sleep: None,
            timeout,
        }
    }

    pub fn start(&mut self) {
        self.sleep = Some(sleep(self.timeout))
    }

    pub fn clear(&mut self) {
        self.sleep = None;
    }
}

impl Future for PingTimeout {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(sleep) = &mut self.sleep {
            sleep.poll_unpin(cx)
        }
        else {
            Poll::Pending
        }
    }
}
