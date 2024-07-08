use std::{
    collections::HashMap,
    ops::{
        Deref,
        DerefMut,
    },
    path::Path,
};

use chrono::{
    DateTime,
    FixedOffset,
};
use serde::{
    Deserialize,
    Serialize,
};
use sqlx::sqlite::SqliteConnectOptions;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
#[error("flows store error")]
pub enum Error {
    Sqlx(#[from] sqlx::Error),
    Migrate(#[from] sqlx::migrate::MigrateError),
    Json(#[from] serde_json::Error),
}

#[derive(Clone, Debug)]
pub struct FlowStore {
    pool: sqlx::SqlitePool,
}

impl FlowStore {
    pub async fn in_memory() -> Result<Self, Error> {
        Self::open_with(SqliteConnectOptions::new()).await
    }

    pub async fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::open_with(SqliteConnectOptions::new().filename(path)).await
    }

    pub async fn create(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::open_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true),
        )
        .await
    }

    async fn open_with(options: SqliteConnectOptions) -> Result<Self, Error> {
        let pool = sqlx::SqlitePool::connect_with(options).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn transaction(&self) -> Result<Transaction<'_>, Error> {
        let transaction = self.pool.begin().await?;
        Ok(Transaction { transaction })
    }
}

#[derive(Debug)]
pub struct Transaction<'a> {
    transaction: sqlx::Transaction<'a, sqlx::Sqlite>,
}

impl<'a> Transaction<'a> {
    pub async fn commit(self) -> Result<(), Error> {
        self.transaction.commit().await?;
        Ok(())
    }

    pub async fn rollback(self) -> Result<(), Error> {
        self.transaction.rollback().await?;
        Ok(())
    }

    pub async fn get_metadata<T: for<'de> Deserialize<'de>>(
        &mut self,
        key: &str,
    ) -> Result<Option<T>, Error> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT value as "value: serde_json::Value"
            FROM metadata
            WHERE key = ?
            "#,
            key
        )
        .fetch_optional(&mut *self.transaction)
        .await?
        else {
            return Ok(None);
        };
        Ok(serde_json::from_value(row.value)?)
    }

    pub async fn put_metadata<T: Serialize>(&mut self, key: &str, value: &T) -> Result<(), Error> {
        let value = serde_json::to_value(value)?;
        sqlx::query!(
            r#"
            INSERT INTO metadata (key, value)
            VALUES (?, ?)
            ON CONFLICT(key) DO UPDATE SET value=excluded.value;
            "#,
            key,
            value,
        )
        .execute(&mut *self.transaction)
        .await?;
        Ok(())
    }

    pub async fn create_flow(
        &mut self,
        flow_id: Uuid,
        destination_address: &str,
        destination_port: u16,
        protocol: u16,
        timestamp: DateTime<FixedOffset>,
        metadata: &Metadata,
    ) -> Result<(), Error> {
        let metadata = serde_json::to_value(metadata)?;
        sqlx::query!(
            r#"
            INSERT INTO flow (flow_id, destination_address, destination_port, protocol, timestamp, metadata)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
            flow_id,
            destination_address,
            destination_port,
            protocol,
            timestamp,
            metadata,
        )
        .execute(&mut *self.transaction)
        .await?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct Metadata {
    inner: HashMap<String, serde_json::Value>,
}

impl Deref for Metadata {
    type Target = HashMap<String, serde_json::Value>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Metadata {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
