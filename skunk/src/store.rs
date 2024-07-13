use std::{
    borrow::Cow,
    path::Path,
};

use sqlx::SqlitePool;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sqlx error")]
    Sqlx(#[from] sqlx::Error),
}

pub enum Location<'a> {
    InMemory,
    Path(&'a Path),
}

impl<'a> Location<'a> {
    pub fn from_option(path: Option<&'a Path>) -> Self {
        if let Some(path) = path {
            Self::Path(path)
        }
        else {
            Self::InMemory
        }
    }

    pub fn as_url(&self) -> Cow<'static, str> {
        match self {
            Location::InMemory => "sqlite::memory:".into(),
            Location::Path(path) => format!("sqlite://{}", path.display()).into(),
        }
    }
}

pub struct Store {
    pool: SqlitePool,
}

impl Store {
    pub async fn open(location: Location<'_>) -> Result<Self, Error> {
        let pool = SqlitePool::connect(&location.as_url()).await?;

        Ok(Self { pool })
    }
}
