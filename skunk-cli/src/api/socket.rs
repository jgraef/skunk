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
    socket::{
        ClientHello,
        ClientMessage,
        ServerHello,
        ServerMessage,
        SocketId,
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
                socket_id: SocketId(Uuid::new_v4()),
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
    socket_id: SocketId,
}

impl Reactor {
    async fn run(mut self) -> Result<(), Error> {
        // register command sender in context
        let (command_tx, mut command_rx) = mpsc::channel(16);
        self.context.connect_socket(Sender {
            tx: command_tx,
            socket_id: self.socket_id,
        });

        // send hello
        self.socket
            .send(&ServerHello {
                server_agent: APP_NAME.into(),
                app_version: APP_VERSION.clone(),
                protocol_version: PROTOCOL_VERSION,
                socket_id: self.socket_id,
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
            ClientMessage::Unsubscribe { subscription_id } => {
                self.context
                    .flows
                    .unsubscribe(self.socket_id, subscription_id)
                    .await;
            }
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
