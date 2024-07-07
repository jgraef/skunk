use axum::{
    extract::{
        ws::Message,
        WebSocketUpgrade,
    },
    response::IntoResponse,
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
    },
    PROTOCOL_VERSION,
};
use tokio::sync::watch;
use tracing::Instrument;

use super::Error;
use crate::app::{
    APP_NAME,
    APP_VERSION,
};

pub(super) async fn handle(
    ws: WebSocketUpgrade,
    reload_rx: Option<watch::Receiver<()>>,
) -> impl IntoResponse {
    let span = tracing::info_span!("websocket");
    ws.on_upgrade(move |socket| {
        async move {
            let reactor = Reactor {
                socket: socket.into(),
                reload_rx,
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
    reload_rx: Option<watch::Receiver<()>>,
}

impl Reactor {
    async fn run(mut self) -> Result<(), Error> {
        // when we start a websocket connection, we want to make sure any previous
        // reloads are ignored.
        if let Some(reload_rx) = &mut self.reload_rx {
            reload_rx.mark_unchanged();
        }

        self.socket
            .send(&ServerHello {
                server_agent: APP_NAME.into(),
                app_version: APP_VERSION.clone(),
                protocol_version: PROTOCOL_VERSION,
            })
            .await?;

        let client_hello: ClientHello = self
            .socket
            .receive()
            .await?
            .ok_or_else(|| Error::Protocol)?;
        tracing::debug!(user_agent = %client_hello.user_agent, "client connected");

        loop {
            tokio::select! {
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
                _ = async {
                    if let Some(reload_rx) = &mut self.reload_rx {
                        if reload_rx.changed().await.is_err() {
                            // sender dropped, so we just set this to None, so in future this future will be pending forever.
                            self.reload_rx = None;
                        }
                    }
                    else {
                        futures_util::future::pending::<()>().await;
                    }
                } => {
                    self.socket.send(&ServerMessage::HotReload).await?;
                }
            }
        }

        Ok(())
    }

    async fn handle_message(&mut self, message: ClientMessage) -> Result<(), Error> {
        match message {
            ClientMessage::SubscribeFlows => todo!(),
            ClientMessage::Start => todo!(),
            ClientMessage::Stop => todo!(),
            ClientMessage::Continue { continue_tx: _ } => todo!(),
        }

        //Ok(())
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

#[derive(Clone, Debug)]
pub struct HotReload {
    pub(super) reload_tx: watch::Sender<()>,
}

impl HotReload {
    pub fn trigger(&self) {
        let _ = self.reload_tx.send(());
    }
}
