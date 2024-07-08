mod context;
mod flows;
mod socket;

use axum::{
    routing,
    Router,
};
use skunk_util::trigger;

pub use self::context::Context;

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
    reload_rx: trigger::Receiver,
}

impl Builder {
    pub fn with_reload_ui(&mut self) -> trigger::Sender {
        let (reload_tx, reload_rx) = trigger::new();
        self.reload_rx = reload_rx;
        reload_tx
    }

    pub fn finish(self) -> Router {
        let context = Context::new(self.reload_rx);

        Router::default()
            .route("/ws", routing::get(socket::handle))
            .nest("/flows", flows::router())
            .route(
                "/settings/tls/ca.cert.pem",
                routing::get(|| async { "TODO" }),
            )
            .fallback(|| async { "404 - Not found" })
            .with_state(context)
    }
}
