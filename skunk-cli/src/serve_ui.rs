use std::{
    path::{
        Path,
        PathBuf,
    },
    sync::Arc,
    task::{
        Context,
        Poll,
    },
};

use axum::{
    body::Body,
    extract::ws::{
        Message,
        WebSocket,
    },
    http::Request,
};
use notify::{
    RecommendedWatcher,
    RecursiveMode,
    Watcher as _,
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
};

#[derive(Clone, Debug)]
pub struct ServeUi {
    inner: ServeDir<ServeFile>,
    watcher: Option<Arc<RecommendedWatcher>>,
}

impl ServeUi {
    pub fn new(path: impl AsRef<Path>, hot_reload: Option<api::HotReload>) -> Self {
        let path = path.as_ref();

        let watcher = if let Some(hot_reload) = hot_reload {
            Some(Arc::new(
                setup_hot_reload(path, hot_reload).expect("Failed to setup hot-reload"),
            ))
        }
        else {
            None
        };

        let inner = ServeDir::new(path).fallback(ServeFile::new_with_mime(
            path.join("index.html"),
            &mime::TEXT_HTML_UTF_8,
        ));

        Self { inner, watcher }
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

fn setup_hot_reload(
    path: &Path,
    hot_reload: api::HotReload,
) -> Result<RecommendedWatcher, notify::Error> {
    // note: the watcher is shutdown when it's dropped.
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        if let Ok(event) = result {
            if event.kind.is_modify() {
                //tracing::debug!("UI modified. Sending reload notification.");
                hot_reload.trigger();
            }
        }
    })?;
    watcher.watch(path, RecursiveMode::Recursive)?;

    Ok(watcher)
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
