use std::{
    io::Cursor,
    path::{
        Path,
        PathBuf,
    },
    time::Duration,
};

use murmur3::murmur3_x64_128;
use notify_async::{
    watch_modified,
    WatchModified,
};

use crate::error::Error;

#[derive(Debug)]
pub struct ConfigFile {
    path: PathBuf,
    watch: WatchModified,
    toml: String,
    hash: FileHash,
}

impl ConfigFile {
    const DEFAULT_DEBOUNCE: Duration = Duration::from_secs(1);

    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();

        let watch = watch_modified(path, Self::DEFAULT_DEBOUNCE).map_err(|source| {
            Error::Watch {
                source,
                path: path.to_owned(),
            }
        })?;

        let toml = std::fs::read_to_string(path).map_err(|source| {
            Error::ReadFile {
                source,
                path: path.to_owned(),
            }
        })?;

        let hash = FileHash::hash(toml.as_bytes());

        Ok(Self {
            path: path.to_owned(),
            watch,
            toml,
            hash,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct FileHash(u128);

impl FileHash {
    pub fn hash(data: &[u8]) -> Self {
        Self(murmur3_x64_128(&mut Cursor::new(data), 0).unwrap())
    }
}
