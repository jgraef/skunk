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

/// skunk's configuration data as parsed from the TOML file.
#[derive(Debug, Deserialize, Default)]
pub struct ConfigData {
    #[serde(default)]
    pub ca: CaConfig,
}

/// skunk's configuration. This includes the parsed configuration and the path
/// to the configuration file.
#[derive(Debug)]
pub struct Config {
    /// The parsed configuration data (from `skunk.toml`).
    pub config: ConfigData,

    /// Path to the configuration directory.
    pub path: PathBuf,
}

impl Config {
    /// Default configuration directory relative to the OS's local configuration
    /// directory (e.g. `~/.config`` on Linux).
    pub const DIR_NAME: &'static str = "gocksec/skunk";

    /// Main configuration file name.
    pub const CONFIG_FILE: &'static str = "skunk.toml";

    /// Open the configuration, either with the given `path` as path to the
    /// configuration directory, or using the default [`Self::DIR_NAME`].
    pub fn open(path: Option<impl AsRef<Path>>) -> Result<Self, Error> {
        // determine path to configuration directory.
        let path = path
            .map(|path| path.as_ref().to_owned())
            .or_else(|| dirs::config_local_dir().map(|path| path.join(Self::DIR_NAME)))
            .ok_or_else(|| eyre!("Could not determine config directory"))?;

        // if the directory doesn't exist, create it.
        if !path.exists() {
            std::fs::create_dir_all(&path)?;
        }

        // parse the configuration TOML file.
        let config_file_path = path.join(Self::CONFIG_FILE);
        let config = if config_file_path.exists() {
            // file exists, so we just parrse it.
            let toml = std::fs::read_to_string(&config_file_path)?;
            toml::from_str(&toml)?
        }
        else {
            // file doesn't exist. create it with default config.
            let config = ConfigData::default();
            std::fs::write(&config_file_path, include_str!("../skunk.default.toml"))?;
            config
        };

        Ok(Self { config, path })
    }

    /// Given a path, return this path relative to the configuration directory.
    pub fn relative_path(&self, path: impl AsRef<Path>) -> PathBuf {
        self.path.join(path.as_ref())
    }
}

/// Deref, so it's slightly easier to access the parsed config's fields.
impl Deref for Config {
    type Target = ConfigData;

    fn deref(&self) -> &Self::Target {
        &self.config
    }
}

#[derive(Debug, Deserialize)]
pub struct CaConfig {
    #[serde(default = "default_config_key_file")]
    pub key_file: PathBuf,

    #[serde(default = "default_config_cert_file")]
    pub cert_file: PathBuf,
}

fn default_config_key_file() -> PathBuf {
    "ca.key.pem".into()
}

fn default_config_cert_file() -> PathBuf {
    "ca.cert.pem".into()
}

impl Default for CaConfig {
    fn default() -> Self {
        Self {
            key_file: default_config_key_file(),
            cert_file: default_config_cert_file(),
        }
    }
}
