use std::{
    borrow::Cow,
    path::{
        Path,
        PathBuf,
    },
    sync::Arc,
    time::Duration,
};

use serde::Deserialize;
use tokio::sync::RwLock;
use toml_edit::DocumentMut;
use tracing::Instrument;

use super::{
    Error,
    FileHash,
    DEFAULT_CONFIG,
};
use crate::util::watch::{
    FileWatcher,
    WatchModified,
};

#[derive(Clone, Debug)]
pub struct ConfigFile {
    inner: Arc<RwLock<Inner>>,
}

pub struct Loader {
    inner: Inner,
    watch: WatchModified,
    hash: FileHash,
}

#[derive(Debug, Deserialize)]
pub struct Data {
    data_dir: Option<PathBuf>,
}

#[derive(Debug)]
struct Inner {
    path: PathBuf,
    hash: FileHash,
    document: DocumentMut,
    data: Data,
}

impl ConfigFile {
    const DEBOUNCE: Duration = Duration::from_secs(1);

    pub fn open(path: impl AsRef<Path>) -> Result<Loader, Error> {
        open(path.as_ref())
    }
}

impl Loader {
    pub fn load(self) -> ConfigFile {
        let path = self.inner.path.clone();
        let inner = Arc::new(RwLock::new(self.inner));

        let span = tracing::info_span!("watch-config");
        tokio::spawn(
            WatchConfig {
                inner: inner.clone(),
                watch: self.watch,
                path,
                hash: self.hash,
            }
            .run()
            .instrument(span),
        );

        ConfigFile { inner }
    }

    pub fn data_dir(&self) -> Option<&Path> {
        self.inner.data.data_dir.as_deref()
    }
}

fn open(path: &Path) -> Result<Loader, Error> {
    let mut toml: Option<Cow<'static, str>> = None;

    if !path.exists() {
        std::fs::write(path, DEFAULT_CONFIG).map_err(|error| {
            Error::WriteFile {
                error,
                path: path.to_owned(),
            }
        })?;

        toml = Some(DEFAULT_CONFIG.into());
    }

    let watcher = FileWatcher::new().map_err(|error| {
        Error::WatchFile {
            error,
            path: path.to_owned(),
        }
    })?;

    let watch = WatchModified::new(watcher, ConfigFile::DEBOUNCE).map_err(|error| {
        Error::WatchFile {
            error,
            path: path.to_owned(),
        }
    })?;

    let toml = if let Some(toml) = toml {
        toml
    }
    else {
        std::fs::read_to_string(path)
            .map_err(|error| {
                Error::ReadFile {
                    error,
                    path: path.to_owned(),
                }
            })?
            .into()
    };

    let hash = FileHash::hash(toml.as_bytes());

    let document: DocumentMut = toml.parse().map_err(|error| {
        Error::ParseToml {
            error,
            path: path.to_owned(),
            toml: toml.clone().into_owned(),
        }
    })?;

    let data: Data = toml_edit::de::from_document(document.clone()).map_err(|error| {
        Error::ParseToml {
            error: error.into(),
            path: path.to_owned(),
            toml: toml.into_owned(),
        }
    })?;

    Ok(Loader {
        inner: Inner {
            path: path.to_owned(),
            hash,
            document,
            data,
        },
        watch,
        hash,
    })
}

#[derive(Debug)]
struct WatchConfig {
    inner: Arc<RwLock<Inner>>,
    watch: WatchModified,
    path: PathBuf,
    hash: FileHash,
}

impl WatchConfig {
    async fn run(mut self) {
        while let Ok(()) = self.watch.wait().await {
            match self.reload() {
                Ok(None) => {}
                Ok(Some(_document)) => todo!(),
                Err(_e) => todo!(),
            }
        }
    }

    fn reload(&mut self) -> Result<Option<DocumentMut>, Error> {
        let toml = std::fs::read_to_string(&self.path).map_err(|error| {
            Error::ReadFile {
                error,
                path: self.path.to_owned(),
            }
        })?;

        let hash = FileHash::hash(toml.as_bytes());

        if hash != self.hash {
            let document: DocumentMut = toml.parse().map_err(|error| {
                Error::ParseToml {
                    error,
                    path: self.path.to_owned(),
                    toml,
                }
            })?;

            self.hash = hash;

            Ok(Some(document))
        }
        else {
            Ok(None)
        }
    }
}
