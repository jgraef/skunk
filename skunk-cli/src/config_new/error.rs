use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not determine config directory.")]
    ConfigDirectory,

    #[error("Could not determine data directory.")]
    DataDirectory,

    #[error("Could not create directory: {path}")]
    CreateDirectory {
        #[source]
        error: std::io::Error,
        path: PathBuf,
    },

    #[error("Could not read file: {path}")]
    ReadFile {
        #[source]
        error: std::io::Error,
        path: PathBuf,
    },

    #[error("Could not write file: {path}")]
    WriteFile {
        #[source]
        error: std::io::Error,
        path: PathBuf,
    },

    #[error("Could not watch file: {path}")]
    WatchFile {
        #[source]
        error: notify::Error,
        path: PathBuf,
    },

    #[error("Could not parse TOML file: {path}")]
    ParseToml {
        #[source]
        error: toml_edit::TomlError,
        path: PathBuf,
        toml: String,
    },
}
