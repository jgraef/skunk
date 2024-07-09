use std::collections::HashMap;

use axum::{
    extract::{
        ws::Message,
        State,
        WebSocketUpgrade,
    },
    response::IntoResponse,
};
use serde::{
    Deserialize,
    Serialize,
};
use skunk_api_protocol::{
    flow::Flow,
    socket::{
        ClientHello,
        ClientMessage,
        ServerHello,
        ServerMessage,
        SocketId,
        SubscriptionId,
    },
    PROTOCOL_VERSION,
};
use tokio::sync::mpsc;
use tracing::Instrument;
use uuid::Uuid;

use super::{
    Context,
    Error,
};
use crate::app::{
    APP_NAME,
    APP_VERSION,
};

pub(super) async fn handle(
    ws: WebSocketUpgrade,
    State(context): State<Context>,
) -> impl IntoResponse {
    let span = tracing::info_span!("websocket");
    ws.on_upgrade(move |socket| {
        async move {
            let reactor = Reactor {
                socket: socket.into(),
                context,
            };

            if let Err(e) = reactor.run().await {
                tracing::error!("{e:?}");
            }
        }
        .instrument(span)
    })
}

struct Reactor {
    socket: WebSocket,
    context: Context,
}

impl Reactor {
    async fn run(mut self) -> Result<(), Error> {
        // register command sender in context
        let (command_tx, mut command_rx) = mpsc::channel(16);
        let socket_id = SocketId(Uuid::new_v4());
        self.context.connect_socket(Sender {
            tx: command_tx,
            socket_id,
        });

        // send hello
        self.socket
            .send(&ServerHello {
                server_agent: APP_NAME.into(),
                app_version: APP_VERSION.clone(),
                protocol_version: PROTOCOL_VERSION,
                socket_id,
            })
            .await?;

        // receive hello
        let client_hello: ClientHello = self
            .socket
            .receive()
            .await?
            .ok_or_else(|| Error::Protocol)?;

        let mut reload_ui = self.context.reload_ui();

        tracing::debug!(user_agent = %client_hello.user_agent, "client connected");

        loop {
            tokio::select! {
                // command
                command_opt = command_rx.recv() => {
                    if let Some(command) = command_opt {
                        self.handle_command(command).await?;
                    }
                    else {
                        tracing::warn!("websocket disconnected from command sender");
                        break;
                    }
                }

                // message from client
                message_res = self.socket.receive::<ClientMessage>() => {
                    if let Some(message) = message_res? {
                        self.handle_message(message).await?;
                    }
                    else {
                        // websocket closed
                        break;
                    }
                }

                // hot-reload signal
                _ = reload_ui.triggered() => {
                    self.socket.send(&ServerMessage::ReloadUi).await?;
                }
            }
        }

        Ok(())
    }

    async fn handle_command(&mut self, command: Command) -> Result<(), Error> {
        match command {
            Command::SendMessage(message) => {
                self.socket.send(&message).await?;
            } // todo
        }

        Ok(())
    }

    async fn handle_message(&mut self, message: ClientMessage) -> Result<(), Error> {
        match message {
            ClientMessage::Ping => {
                self.socket.send(&ServerMessage::Pong).await?;
            }
            ClientMessage::SubscribeFlows => todo!(),
            ClientMessage::Start => todo!(),
            ClientMessage::Stop => todo!(),
            ClientMessage::Continue { .. } => todo!(),
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum Command {
    SendMessage(ServerMessage),
    // todo
}

#[derive(Clone, Debug)]
pub struct Sender {
    tx: mpsc::Sender<Command>,
    socket_id: SocketId,
}

impl Sender {
    async fn send_command(&mut self, command: Command) -> Result<(), Closed> {
        self.tx.send(command).await.map_err(|_| Closed)
    }

    pub async fn send_message(&mut self, message: ServerMessage) -> Result<(), Closed> {
        self.send_command(Command::SendMessage(message)).await
    }

    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    pub fn socket_id(&self) -> SocketId {
        self.socket_id
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Websocket connection closed")]
pub struct Closed;

#[derive(Debug, Default)]
pub struct Subscriptions {
    inner: HashMap<(SocketId, SubscriptionId), Sender>,
}

impl Subscriptions {
    pub fn insert(&mut self, subscription_id: SubscriptionId, socket: Sender) {
        self.inner
            .insert((socket.socket_id, subscription_id), socket);
    }

    async fn for_each(
        &mut self,
        mut f: impl FnMut(SubscriptionId) -> ServerMessage,
    ) -> Result<(), Error> {
        let mut remove = vec![];

        for ((socket_id, subscription_id), sender) in self.inner.iter_mut() {
            let message = f(*subscription_id);

            if let Err(Closed) = sender.send_message(message).await {
                remove.push((*socket_id, *subscription_id));
            }
        }

        for key in remove {
            self.inner.remove(&key);
        }

        Ok(())
    }

    pub async fn begin_flow(&mut self, flow: &Flow) -> Result<(), Error> {
        self.for_each(|subscription_id| {
            ServerMessage::BeginFlow {
                subscription_id,
                flow: flow.clone(),
            }
        })
        .await
    }
}

// Wrapper around axum's WebSocket to send and receive msgpack-encoded messages
struct WebSocket {
    inner: axum::extract::ws::WebSocket,
}

impl From<axum::extract::ws::WebSocket> for WebSocket {
    fn from(inner: axum::extract::ws::WebSocket) -> Self {
        Self { inner }
    }
}

impl WebSocket {
    async fn receive<T: for<'de> Deserialize<'de>>(&mut self) -> Result<Option<T>, Error> {
        while let Some(message) = self.inner.recv().await.transpose()? {
            match message {
                Message::Binary(data) => {
                    let item: T = rmp_serde::from_slice(&data)?;
                    return Ok(Some(item));
                }
                Message::Close(_) => return Ok(None),
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
