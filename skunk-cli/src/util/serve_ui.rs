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
    http::Request,
};
use notify_async::watch_modified;
use skunk_util::trigger;
use tower_http::services::{
    ServeDir,
    ServeFile,
};
use tower_service::Service;

use crate::{
    api,
    env::{
        config::UiConfig,
        Environment,
        Error,
    },
};

#[derive(Clone, Debug)]
pub struct ServeUi {
    inner: ServeDir<ServeFile>,
}

impl ServeUi {
    pub fn new(path: impl AsRef<Path>, reload_ui: Option<trigger::Sender>) -> Self {
        let path = path.as_ref();

        if let Some(reload_ui) = reload_ui {
            let mut watch = watch_modified(path, Duration::from_secs(2))
                .expect("Failed to watch for file changes");
            tokio::spawn(async move {
                while let Ok(()) = watch.modified().await {
                    tracing::info!("UI modified. Triggering reload");
                    reload_ui.trigger();
                }
            });
        }

        let inner = ServeDir::new(path).fallback(ServeFile::new_with_mime(
            path.join("index.html"),
            &mime::TEXT_HTML_UTF_8,
        ));

        Self { inner }
    }

    pub async fn from_environment(
        environment: &Environment,
        api_builder: &mut api::Builder,
    ) -> Result<Self, Error> {
        let ui_config = environment
            .get_untracked::<UiConfig>("ui")
            .await?
            .unwrap_or_default();
        let ui_dev_env_var_set = std::env::var("SKUNK_UI_DEV").is_ok();

        let ui_path = if ui_dev_env_var_set {
            PathBuf::from(std::env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("skunk-ui")
                .join("dist")
                .canonicalize()
                .expect("Could not get absolute path for UI")
        }
        else {
            environment.data_relative_path(&ui_config.path)
        };

        let reload_ui =
            (ui_config.auto_reload || ui_dev_env_var_set).then(|| api_builder.with_reload_ui());

        tracing::info!(path = %ui_path.display(), auto_reload = reload_ui.is_some(), "serving ui");
        Ok(Self::new(ui_path, reload_ui))
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
