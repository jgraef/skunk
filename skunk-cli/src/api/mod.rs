mod flows;
mod socket;

use std::{
    collections::HashMap,
    sync::Arc,
};

use axum::{
    routing,
    Router,
};
use flows::Flows;
use parking_lot::RwLock;
use skunk_api_protocol::{
    error::{
        ApiError,
        NoSuchSocket,
    },
    socket::SocketId,
};
use skunk_flows_store::FlowStore;
use skunk_util::trigger;

pub const SERVER_AGENT: &'static str = std::env!("CARGO_PKG_NAME");

#[derive(Debug, thiserror::Error)]
#[error("API server error")]
pub enum Error {
    Axum(#[from] axum::Error),
    Decode(#[from] rmp_serde::decode::Error),
    Encode(#[from] rmp_serde::encode::Error),
    Protocol,
    FlowStore(#[from] skunk_flows_store::Error),
}

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        Self::internal(error)
    }
}

pub fn builder() -> Builder {
    Builder::default()
}

#[derive(Debug, Default)]
pub struct Builder {
    reload_ui: trigger::Receiver,
    flow_store: Option<FlowStore>,
}

impl Builder {
    pub fn with_reload_ui(&mut self) -> trigger::Sender {
        let (reload_tx, reload_rx) = trigger::new();
        self.reload_ui = reload_rx;
        reload_tx
    }

    pub fn with_flow_store(mut self, flow_store: FlowStore) -> Self {
        self.flow_store = Some(flow_store);
        self
    }
}

impl Builder {
    pub fn finish(self) -> Router {
        let context = Context {
            sockets: Arc::new(RwLock::new(HashMap::new())),
            reload_ui: Arc::new(self.reload_ui),
            flows: Flows::new(self.flow_store),
        };

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

#[derive(Clone, Debug)]
pub struct Context {
    sockets: Arc<RwLock<HashMap<SocketId, socket::Sender>>>,
    reload_ui: Arc<trigger::Receiver>,
    flows: Flows,
}

impl Context {
    pub fn connect_socket(&self, sender: socket::Sender) {
        let mut sockets = self.sockets.write();
        sockets.insert(sender.socket_id(), sender);
    }

    pub fn socket(&self, id: SocketId) -> Result<socket::Sender, NoSuchSocket> {
        let sockets = self.sockets.read();

        let socket = sockets
            .get(&id)
            .cloned()
            .ok_or_else(|| NoSuchSocket { id })?;

        // check if socket is closed. if it is, remove from hashmap
        if socket.is_closed() {
            // race-condition here doesn't matter. don't care if another thread removes the
            // socket first.
            drop(sockets);
            let mut sockets = self.sockets.write();
            sockets.remove(&id);
            return Err(NoSuchSocket { id });
        }

        Ok(socket)
    }

    pub fn reload_ui(&self) -> trigger::Receiver {
        (*self.reload_ui).clone()
    }
}
