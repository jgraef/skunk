use std::path::{
    Path,
    PathBuf,
};

use color_eyre::eyre::{
    eyre,
    Error,
};
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct ConfigData {
    #[serde(default)]
    pub ca: CaConfig,
}

#[derive(Debug)]
pub struct Config {
    pub config: ConfigData,
    pub path: PathBuf,
}

impl Config {
    pub const DIR_NAME: &'static str = "gocksec/skunk";
    pub const CONFIG_FILE: &'static str = "skunk.toml";

    pub fn open(path: Option<impl AsRef<Path>>) -> Result<Self, Error> {
        let path = path
            .map(|path| path.as_ref().to_owned())
            .or_else(|| dirs::config_local_dir().map(|path| path.join(Self::DIR_NAME)))
            .ok_or_else(|| eyre!("Could not determine config directory"))?;

        if !path.exists() {
            std::fs::create_dir_all(&path)?;
        }

        let config_file_path = path.join(Self::CONFIG_FILE);
        let config = if config_file_path.exists() {
            let toml = std::fs::read_to_string(&config_file_path)?;
            toml::from_str(&toml)?
        }
        else {
            let config = ConfigData::default();
            std::fs::write(&config_file_path, include_str!("../skunk.default.toml"))?;
            config
        };

        Ok(Self { config, path })
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
