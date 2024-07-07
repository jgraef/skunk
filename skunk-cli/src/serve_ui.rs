use std::{
    path::{
        Path,
        PathBuf,
    },
    task::{
        Context,
        Poll,
    },
    time::Duration,
};

use axum::{
    body::Body,
    extract::ws::{
        Message,
        WebSocket,
    },
    http::Request,
};
use tokio::sync::watch;
use tower_http::services::{
    ServeDir,
    ServeFile,
};
use tower_service::Service;

use crate::{
    api,
    config::Config,
    util::watch::watch_modified,
};

#[derive(Clone, Debug)]
pub struct ServeUi {
    inner: ServeDir<ServeFile>,
}

impl ServeUi {
    pub fn new(path: impl AsRef<Path>, hot_reload: Option<api::HotReload>) -> Self {
        let path = path.as_ref();

        if let Some(hot_reload) = hot_reload {
            let mut watch = watch_modified(path, Duration::from_secs(2))
                .expect("Failed to watch for file changes");
            tokio::spawn(async move {
                while let Ok(()) = watch.wait().await {
                    tracing::info!("UI modified. Triggering reload");
                    hot_reload.trigger();
                }
            });
        }

        let inner = ServeDir::new(path).fallback(ServeFile::new_with_mime(
            path.join("index.html"),
            &mime::TEXT_HTML_UTF_8,
        ));

        Self { inner }
    }

    pub fn from_config(config: &Config, api_builder: &mut api::Builder) -> Self {
        if std::env::var("SKUNK_UI_DEV").is_ok() {
            let path = PathBuf::from(std::env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("skunk-ui")
                .join("dist")
                .canonicalize()
                .unwrap();
            let hot_reload = api_builder.with_hot_reload();

            tracing::info!(path = %path.display(), "serving ui from workspace with hot-reload");
            Self::new(path, Some(hot_reload))
        }
        else {
            let path = config.data_relative_path(&config.ui.path);

            if !path.exists() {
                todo!("Install UI");
            }

            tracing::info!(path = %path.display(), "serving ui");
            Self::new(path, None)
        }
    }
}

impl Service<Request<Body>> for ServeUi {
    type Response = <ServeDir<ServeFile> as Service<Request<Body>>>::Response;
    type Error = <ServeDir<ServeFile> as Service<Request<Body>>>::Error;
    type Future = <ServeDir<ServeFile> as Service<Request<Body>>>::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        <ServeDir<ServeFile> as Service<Request<Body>>>::poll_ready(&mut self.inner, cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        <ServeDir<ServeFile> as Service<Request<Body>>>::call(&mut self.inner, req)
    }
}

async fn reload_handler(mut socket: WebSocket, mut reload_rx: watch::Receiver<()>) {
    let reload_message = "{\"reload\": true}";

    tracing::debug!("connected");

    loop {
        tokio::select! {
            message_res = socket.recv() => {
                if message_res.transpose().ok().flatten().is_none() {
                    //tracing::debug!("peer disconnected");
                    break;
                }
            },
            changed_res = reload_rx.changed() => {
                tracing::debug!("sending reload-notification");

                if let Err(_e) = changed_res {
                    tracing::warn!("reload_tx dropped"); // why does it drop????
                    break;
                }
                tracing::debug!("notify");
                if socket.send(Message::Text(reload_message.to_owned())).await.is_err() {
                    //tracing::debug!("send failed");
                    break;
                }
            }
        }
    }

    tracing::debug!("disconnected");
}
