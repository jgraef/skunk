mod websocket;

use axum::{
    extract::WebSocketUpgrade,
    routing,
    Router,
};
use tokio::sync::watch;

pub use self::websocket::HotReload;
use crate::util::msgpack::Msgpack;

pub const SERVER_AGENT: &'static str = std::env!("CARGO_PKG_NAME");

#[derive(Debug, thiserror::Error)]
#[error("API server error")]
pub enum Error {
    Axum(#[from] axum::Error),
    Decode(#[from] rmp_serde::decode::Error),
    Encode(#[from] rmp_serde::encode::Error),
    Protocol,
}

pub fn builder() -> Builder {
    Builder::default()
}

#[derive(Debug, Default)]
pub struct Builder {
    reload_rx: Option<watch::Receiver<()>>,
}

impl Builder {
    pub fn with_hot_reload(&mut self) -> HotReload {
        let (reload_tx, reload_rx) = watch::channel(());
        self.reload_rx = Some(reload_rx);
        HotReload { reload_tx }
    }

    pub fn finish(self) -> Router {
        let Self { reload_rx } = self;

        Router::default()
            .route(
                "/ws",
                routing::get(move |ws: WebSocketUpgrade| websocket::handle(ws, reload_rx)),
            )
            .route("/flows", routing::get(get_flows))
            .route(
                "/settings/tls/ca.cert.pem",
                routing::get(|| async { "TODO" }),
            )
            .fallback(|| async { "404 - Not found" })
    }
}

async fn get_flows() -> Msgpack<()> {
    ().into() // todo
}
