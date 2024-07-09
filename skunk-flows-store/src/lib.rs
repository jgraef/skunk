use std::path::Path;

use chrono::{
    DateTime,
    FixedOffset,
};
use serde::{
    Deserialize,
    Serialize,
};
use skunk_api_protocol::flows::{
    Flow,
    FlowId,
    Metadata,
    ProtocolId,
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
        .fetch_optional(self.transaction.as_mut())
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
        .execute(self.transaction.as_mut())
        .await?;
        Ok(())
    }

    pub async fn create_flow(&mut self, flow: &Flow) -> Result<(), Error> {
        let protocol = flow.protocol.map(|protocol| protocol.0);
        let metadata = serde_json::to_value(&flow.metadata)?;
        sqlx::query!(
            r#"
            INSERT INTO flow (flow_id, destination_address, destination_port, protocol, timestamp, metadata)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
            flow.flow_id.0,
            flow.destination_address,
            flow.destination_port,
            protocol,
            flow.timestamp,
            metadata,
        )
        .execute(self.transaction.as_mut())
        .await?;
        Ok(())
    }

    pub async fn get_flows(
        &mut self,
        after: Option<DateTime<FixedOffset>>,
        before: Option<DateTime<FixedOffset>>,
    ) -> Result<Vec<Flow>, Error> {
        sqlx::query!(
            r#"
            SELECT
                flow_id as "flow_id: Uuid",
                destination_address,
                destination_port as "destination_port: u16",
                protocol as "protocol: Uuid",
                timestamp as "timestamp: DateTime<FixedOffset>",
                metadata as "metadata: serde_json::Value"
            FROM flow
            WHERE
                ? > timestamp
                AND timestamp < ?
            "#,
            after,
            before,
        )
        .fetch_all(self.transaction.as_mut())
        .await?
        .into_iter()
        .map(|row| {
            Ok(Flow {
                flow_id: FlowId(row.flow_id),
                destination_address: row.destination_address,
                destination_port: row.destination_port,
                protocol: row.protocol.map(ProtocolId),
                timestamp: row.timestamp,
                metadata: metadata_from_row(row.metadata)?,
            })
        })
        .collect::<Result<Vec<Flow>, Error>>()
    }
}

fn metadata_from_row(metadata: Option<serde_json::Value>) -> Result<Metadata, Error> {
    Ok(metadata
        .map(|metadata| serde_json::from_value(metadata))
        .transpose()?
        .unwrap_or_default())
}
