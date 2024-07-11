pub mod args;
pub mod config;
mod error;

use std::{
    io::Cursor,
    path::{
        Path,
        PathBuf,
    },
};

use config::ConfigFile;
use murmur3::murmur3_x64_128;
use serde::Deserialize;

pub use self::{
    args::{
        Args,
        Options,
    },
    error::Error,
};

/// Default configuration directory relative to the OS's local configuration
/// directory (e.g. `~/.config`` on Linux).
pub const CONFIG_DIR_NAME: &'static str = "feralsec/skunk";

pub const DATA_DIR_NAME: &'static str = "feralsec/skunk";

/// Main configuration file name.
pub const CONFIG_FILE: &'static str = "skunk.toml";

pub const DEFAULT_CONFIG: &'static str = include_str!("skunk.default.toml");

#[derive(Clone, Debug)]
pub struct Environment {
    options: Options,
    config_dir: PathBuf,
    data_dir: PathBuf,
    config_file: ConfigFile,
}

impl Environment {
    /// Open the configuration, either with the command-line-specified path to
    /// the configuration directory, or using the default
    /// [`CONFIG_DIR_NAME`].
    pub fn from_options(options: Options) -> Result<Self, Error> {
        // determine path to configuration directory.
        let config_dir = options
            .config
            .as_ref()
            .map(|path| path.to_owned())
            .or_else(|| dirs::config_local_dir().map(|path| path.join(CONFIG_DIR_NAME)))
            .ok_or(Error::ConfigDirectory)?;

        // if the directory doesn't exist, create it.
        create_dir_all(&config_dir)?;

        // parse the configuration TOML file.
        let config_file_path = config_dir.join(CONFIG_FILE);
        let config_file = ConfigFile::open(config_file_path)?;

        // determine path to data directory
        let data_dir = config_file
            .data_dir()?
            .or_else(|| dirs::data_local_dir().map(|path| path.join(DATA_DIR_NAME)))
            .ok_or(Error::DataDirectory)?;

        create_dir_all(&data_dir)?;

        Ok(Self {
            options,
            config_dir,
            data_dir,
            config_file: config_file.load(),
        })
    }

    pub async fn get_untracked<T: for<'de> Deserialize<'de>>(
        &self,
        key: &str,
    ) -> Result<Option<T>, Error> {
        self.config_file.get_untracked(key).await
    }

    pub fn config_relative_path(&self, path: impl AsRef<Path>) -> PathBuf {
        self.config_dir.join(path)
    }

    pub fn data_relative_path(&self, path: impl AsRef<Path>) -> PathBuf {
        self.data_dir.join(path)
    }
}

fn create_dir_all(path: impl AsRef<Path>) -> Result<(), Error> {
    let path = path.as_ref();
    std::fs::create_dir_all(path).map_err(|error| {
        Error::CreateDirectory {
            error,
            path: path.to_owned(),
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct FileHash(u128);

impl FileHash {
    pub fn hash(data: &[u8]) -> Self {
        Self(murmur3_x64_128(&mut Cursor::new(data), 0).unwrap())
    }
}
