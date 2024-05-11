use std::path::{
    Path,
    PathBuf,
};

use serde::Deserialize;

use crate::core::tls::CaConfig;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("toml error")]
    Toml(#[from] toml::de::Error),

    #[error("Could not determine configuration path.")]
    NoConfigPath,
}

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
            .ok_or(Error::NoConfigPath)?;

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
            std::fs::write(&config_file_path, include_str!("../../skunk.default.toml"))?;
            config
        };

        Ok(Self { config, path })
    }
}
