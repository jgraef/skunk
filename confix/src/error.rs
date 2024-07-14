use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to watch file: {path}")]
    Watch {
        #[source]
        source: notify_async::Error,
        path: PathBuf,
    },

    #[error("Failed to read file: {path}")]
    ReadFile {
        #[source]
        source: std::io::Error,
        path: PathBuf,
    },
}
