mod capture;
mod flow;
mod socket;

use std::{
    collections::HashMap,
    sync::Arc,
};

use axum::{
    body::Body,
    extract::State,
    http::{
        header,
        StatusCode,
    },
    response::{
        IntoResponse,
        Response,
    },
    routing,
    Router,
};
use flow::Flows;
use parking_lot::RwLock;
use skunk_api_protocol::{
    error::{
        ApiError,
        NoSuchSocket,
    },
    socket::SocketId,
};
use skunk_flow_store::FlowStore;
use skunk_util::trigger;

use crate::env::{
    config::TlsConfig,
    Environment,
};

pub const SERVER_AGENT: &'static str = std::env!("CARGO_PKG_NAME");

#[derive(Debug, thiserror::Error)]
#[error("API server error")]
pub enum Error {
    Axum(#[from] axum::Error),
    Decode(#[from] rmp_serde::decode::Error),
    Encode(#[from] rmp_serde::encode::Error),
    Protocol,
    FlowStore(#[from] skunk_flow_store::Error),
}

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        Self::internal(error)
    }
}

pub fn builder(env: Environment) -> Builder {
    Builder {
        env,
        reload_ui: Default::default(),
        flow_store: None,
    }
}

#[derive(Debug)]
pub struct Builder {
    env: Environment,
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
            env: self.env,
            sockets: Arc::new(RwLock::new(HashMap::new())),
            reload_ui: Arc::new(self.reload_ui),
            flows: Flows::new(self.flow_store),
        };

        Router::default()
            .route("/ws", routing::get(socket::handle))
            .nest("/flow", flow::router())
            .nest("/capture", capture::router())
            .route("/feralsec-root-cert.pem", routing::get(get_tls_root_cert))
            .fallback(|| async { "404 - Not found" })
            .with_state(context)
    }
}

#[derive(Clone, Debug)]
pub struct Context {
    env: Environment,
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

async fn get_tls_root_cert(State(context): State<Context>) -> impl IntoResponse {
    async fn load_file(env: &Environment) -> Result<Option<Vec<u8>>, crate::Error> {
        let tls_config = env
            .get_untracked::<TlsConfig>("tls")
            .await?
            .unwrap_or_default();
        let cert_file = env.config_relative_path(&tls_config.cert_file);
        if cert_file.exists() {
            Ok(Some(std::fs::read(&cert_file)?))
        }
        else {
            Ok(None)
        }
    }

    match load_file(&context.env).await {
        Ok(None) => (StatusCode::NOT_FOUND, "Not found").into_response(),
        Ok(Some(contents)) => {
            Response::builder()
                .header(
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"feralsec-root-cert.pem\"",
                )
                .header(header::CONTENT_LENGTH, contents.len())
                .header(header::CONTENT_TYPE, mime::TEXT_PLAIN.as_ref())
                .body(Body::from(contents))
                .expect("Tried to construct an invalid HTTP response")
        }
        Err(e) => {
            tracing::error!("Error while trying to serve root certificate: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
        }
    }
}
