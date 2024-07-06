use axum::{
    extract::{
        ws::Message,
        WebSocketUpgrade,
    },
    routing,
    Router,
};
use serde::{
    Deserialize,
    Serialize,
};
use tracing::Instrument;

use crate::protocol::ClientMessage;

#[derive(Debug, thiserror::Error)]
#[error("API server error")]
pub enum Error {
    Axum(#[from] axum::Error),
    Decode(#[from] rmp_serde::decode::Error),
    Encode(#[from] rmp_serde::encode::Error),
}

pub fn router() -> Router {
    Router::default().route(
        "/ws",
        routing::get(|ws: WebSocketUpgrade| {
            async move {
                let span = tracing::info_span!("websocket");
                ws.on_upgrade(move |socket| {
                    async move {
                        if let Err(e) = handle_websocket(socket.into()).await {
                            tracing::error!("{e:?}");
                        }
                    }
                    .instrument(span)
                })
            }
        }),
    )
}

async fn handle_websocket(mut socket: WebSocket) -> Result<(), Error> {
    while let Some(message) = socket.receive::<ClientMessage>().await? {
        match message {
            ClientMessage::SubscribeFlows => todo!(),
            ClientMessage::Start => todo!(),
            ClientMessage::Stop => todo!(),
            ClientMessage::Continue { continue_tx: _ } => todo!(),
        }
    }

    Ok(())
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
