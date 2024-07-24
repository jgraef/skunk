use std::{
    borrow::Cow,
    path::{
        Path,
        PathBuf,
    },
    sync::Arc,
    time::Duration,
};

use notify_async::{
    WatchFiles,
    WatchModified,
};
use serde::{
    de::IntoDeserializer,
    Deserialize,
};
use tokio::sync::RwLock;
use toml_edit::DocumentMut;
use tracing::Instrument;

use super::{
    Error,
    FileHash,
    DEFAULT_CONFIG,
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

#[derive(Debug)]
struct Inner {
    path: PathBuf,
    hash: FileHash,
    document: DocumentMut,
}

impl Inner {
    pub fn get_untracked<T: for<'de> Deserialize<'de>>(
        &self,
        key: &str,
    ) -> Result<Option<T>, Error> {
        let Some(item) = self.document.get(key)
        else {
            return Ok(None);
        };
        let Ok(value) = item.clone().into_value()
        else {
            // todo: return an error here?
            return Ok(None);
        };
        let deserializer = value.into_deserializer();
        let value = T::deserialize(deserializer).map_err(|error| {
            Error::ParseToml {
                error: Box::new(error.into()),
                path: self.path.clone(),
                toml: item.to_string(),
            }
        })?;
        Ok(Some(value))
    }
}

impl ConfigFile {
    const DEBOUNCE: Duration = Duration::from_secs(1);

    pub fn open(path: impl AsRef<Path>) -> Result<Loader, Error> {
        open(path.as_ref())
    }

    pub async fn get_untracked<T: for<'de> Deserialize<'de>>(
        &self,
        key: &str,
    ) -> Result<Option<T>, Error> {
        let inner = self.inner.read().await;
        inner.get_untracked(key)
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

    pub fn data_dir(&self) -> Result<Option<PathBuf>, Error> {
        self.inner.get_untracked::<PathBuf>("data_dir")
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

    let watcher = WatchFiles::new().map_err(|error| {
        Error::WatchFile {
            error,
            path: path.to_owned(),
        }
    })?;

    let watch = WatchModified::new(watcher)
        .map_err(|error| {
            Error::WatchFile {
                error,
                path: path.to_owned(),
            }
        })?
        .with_debounce(ConfigFile::DEBOUNCE);

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
            error: Box::new(error),
            path: path.to_owned(),
            toml: toml.clone().into_owned(),
        }
    })?;

    Ok(Loader {
        inner: Inner {
            path: path.to_owned(),
            hash,
            document,
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
        while let Ok(()) = self.watch.modified().await {
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
                    error: Box::new(error),
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

#[derive(Debug, Deserialize)]
pub struct TlsConfig {
    #[serde(default = "default_tls_config_key_file")]
    pub key_file: PathBuf,

    #[serde(default = "default_tls_config_cert_file")]
    pub cert_file: PathBuf,
}

fn default_tls_config_key_file() -> PathBuf {
    "ca.key.pem".into()
}

fn default_tls_config_cert_file() -> PathBuf {
    "ca.cert.pem".into()
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            key_file: default_tls_config_key_file(),
            cert_file: default_tls_config_cert_file(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_ui_config_path")]
    pub path: PathBuf,

    #[serde(default)]
    pub auto_reload: bool,
}

fn default_ui_config_path() -> PathBuf {
    "ui".into()
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            path: default_ui_config_path(),
            auto_reload: false,
        }
    }
}
