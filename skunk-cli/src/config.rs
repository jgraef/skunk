use std::{
    ops::Deref,
    path::{
        Path,
        PathBuf,
    },
};

use color_eyre::eyre::{
    eyre,
    Error,
};
use serde::Deserialize;

/// skunk's configuration. This includes the parsed configuration and the path
/// to the configuration file.
#[derive(Debug)]
pub struct Config {
    /// The parsed configuration data (from `skunk.toml`).
    pub config: ConfigData,

    /// Path to the configuration directory.
    pub config_dir: PathBuf,

    /// Path to the data directory
    pub data_dir: PathBuf,
}

impl Config {
    /// Default configuration directory relative to the OS's local configuration
    /// directory (e.g. `~/.config`` on Linux).
    pub const DIR_NAME: &'static str = "gocksec/skunk";

    /// Main configuration file name.
    pub const CONFIG_FILE: &'static str = "skunk.toml";

    /// Open the configuration, either with the given `path` as path to the
    /// configuration directory, or using the default [`Self::DIR_NAME`].
    pub fn open(config_dir: Option<impl AsRef<Path>>) -> Result<Self, Error> {
        // determine path to configuration directory.
        let config_dir = config_dir
            .map(|path| path.as_ref().to_owned())
            .or_else(|| dirs::config_local_dir().map(|path| path.join(Self::DIR_NAME)))
            .ok_or_else(|| eyre!("Could not determine config directory"))?;

        // if the directory doesn't exist, create it.
        if !config_dir.exists() {
            std::fs::create_dir_all(&config_dir)?;
        }

        // parse the configuration TOML file.
        let config_file_path = config_dir.join(Self::CONFIG_FILE);
        let config: ConfigData = if config_file_path.exists() {
            // file exists, so we just parrse it.
            let toml = std::fs::read_to_string(&config_file_path)?;
            toml::from_str(&toml)?
        }
        else {
            // file doesn't exist. create it with default config.
            const DEFAULT_CONFIG: &'static str = include_str!("../skunk.default.toml");
            std::fs::write(&config_file_path, DEFAULT_CONFIG)?;
            toml::from_str(DEFAULT_CONFIG)?
        };

        // determine path to data directory
        let data_dir = config
            .data_dir
            .clone()
            .or_else(|| dirs::data_local_dir().map(|path| path.join(Self::DIR_NAME)))
            .ok_or_else(|| eyre!("Could not determine data directory"))?;

        Ok(Self {
            config,
            config_dir,
            data_dir,
        })
    }

    /// Given a path, return this path relative to the configuration directory.
    pub fn config_relative_path(&self, path: impl AsRef<Path>) -> PathBuf {
        self.config_dir.join(path.as_ref())
    }

    /// Given a path, return this path relative to the data directory.
    pub fn data_relative_path(&self, path: impl AsRef<Path>) -> PathBuf {
        self.data_dir.join(path.as_ref())
    }
}

/// Deref, so it's slightly easier to access the parsed config's fields.
impl Deref for Config {
    type Target = ConfigData;

    fn deref(&self) -> &Self::Target {
        &self.config
    }
}

/// skunk's configuration data as parsed from the TOML file.
#[derive(Debug, Deserialize, Default)]
pub struct ConfigData {
    pub data_dir: Option<PathBuf>,

    #[serde(default)]
    pub tls: TlsConfig,

    #[serde(default)]
    pub ui: UiConfig,
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
}

fn default_ui_config_path() -> PathBuf {
    "ui".into()
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            path: default_ui_config_path(),
        }
    }
}
