use std::path::Path;

use chrono::{
    DateTime,
    FixedOffset,
};
use serde::{
    Deserialize,
    Serialize,
};
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
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let pool =
            sqlx::SqlitePool::connect(&format!("sqlite:{}", path.as_ref().display())).await?;

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
        _flow_id: Uuid,
        _destination_address: &str,
        _destination_port: u16,
        _protocol: u16,
        _timestamp: DateTime<FixedOffset>,
    ) -> Result<(), Error> {
        todo!();
    }
}
