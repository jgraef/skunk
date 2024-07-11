use std::path::Path;

use chrono::{
    DateTime,
    FixedOffset,
};
use semver::Version;
use semver_macro::version;
use serde::{
    Deserialize,
    Serialize,
};
use skunk_api_protocol::flow::{
    Flow,
    FlowId,
    Message,
    MessageData,
    MessageId,
    MessageKind,
    Metadata,
};
use sqlx::{
    sqlite::SqliteConnectOptions,
    types::Json,
};

pub const FORMAT_VERSION: Version = version!("0.1.0");

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

        let mut transaction = Transaction {
            transaction: pool.begin().await?,
        };

        let _format_version = transaction
            .get_or_set_metadata("format_version", || FORMAT_VERSION.clone())
            .await?;
        // todo: check format version

        transaction.commit().await?;

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

    pub async fn get_metadata<T: for<'de> Deserialize<'de> + Unpin + Send>(
        &mut self,
        key: &str,
    ) -> Result<Option<T>, Error> {
        struct Row<T> {
            value: Json<T>,
        }

        Ok(sqlx::query_as!(
            Row,
            r#"
            SELECT value AS "value: _"
            FROM metadata
            WHERE key = ?
            "#,
            key
        )
        .fetch_optional(self.transaction.as_mut())
        .await?
        .map(|row| row.value.0))
    }

    pub async fn set_metadata<T: Serialize + Sync>(
        &mut self,
        key: &str,
        value: &T,
    ) -> Result<(), Error> {
        let value = Json(value);

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

    pub async fn get_or_set_metadata<T, F>(&mut self, key: &str, f: F) -> Result<T, Error>
    where
        T: Serialize + for<'de> Deserialize<'de> + Send + Sync + Unpin,
        F: FnOnce() -> T,
    {
        if let Some(value) = self.get_metadata(key).await? {
            Ok(value)
        }
        else {
            let value = f();
            self.set_metadata(key, &value).await?;
            Ok(value)
        }
    }

    pub async fn insert_flow(&mut self, flow: &Flow) -> Result<(), Error> {
        sqlx::query!(
            r#"
            INSERT INTO flow (flow_id, parent_id, protocol, timestamp, metadata)
            VALUES (?, ?, ?, ?, ?)
            "#,
            flow.flow_id,
            flow.parent,
            flow.protocol,
            flow.timestamp,
            flow.metadata,
        )
        .execute(self.transaction.as_mut())
        .await?;
        Ok(())
    }

    pub async fn insert_message(&mut self, message: &Message) -> Result<(), Error> {
        sqlx::query!(
            r#"
            INSERT INTO message (message_id, flow_id, kind, timestamp, data, metadata)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
            message.message_id,
            message.flow_id,
            message.kind,
            message.timestamp,
            message.data,
            message.metadata,
        )
        .execute(self.transaction.as_mut())
        .await?;
        Ok(())
    }

    pub async fn get_flows(
        &mut self,
        parent_id: Option<FlowId>,
        after: Option<DateTime<FixedOffset>>,
        before: Option<DateTime<FixedOffset>>,
        limit: Option<usize>,
    ) -> Result<Vec<Flow>, Error> {
        // note: a negative value in the LIMIT clause will cause sqlite to ignore the
        // limit
        let limit = limit
            .and_then(|limit| i32::try_from(limit).ok())
            .unwrap_or(-1);

        sqlx::query!(
            r#"
            SELECT
                flow_id AS "flow_id: FlowId",
                parent_id AS "parent_id: FlowId",
                protocol AS "protocol: String",
                timestamp AS "timestamp: DateTime<FixedOffset>",
                metadata AS "metadata!: Metadata"
            FROM flow
            WHERE
                (parent_id = ?1 OR ?1 IS NULL)
                AND
                (?2 > timestamp OR ?2 IS NULL)
                AND
                (timestamp < ?3 OR ?3 IS NULL)
            ORDER BY timestamp ASC
            LIMIT ?4
            "#,
            parent_id,
            after,
            before,
            limit,
        )
        .fetch_all(self.transaction.as_mut())
        .await?
        .into_iter()
        .map(|row| {
            Ok(Flow {
                flow_id: row.flow_id,
                parent: row.parent_id,
                protocol: row.protocol,
                timestamp: row.timestamp,
                metadata: row.metadata,
            })
        })
        .collect::<Result<Vec<Flow>, Error>>()
    }

    pub async fn get_messages(
        &mut self,
        flow_id: Option<FlowId>,
        after: Option<DateTime<FixedOffset>>,
        before: Option<DateTime<FixedOffset>>,
        limit: Option<usize>,
    ) -> Result<Vec<Message>, Error> {
        // note: a negative value in the LIMIT clause will cause sqlite to ignore the
        // limit
        let limit = limit
            .and_then(|limit| i32::try_from(limit).ok())
            .unwrap_or(-1);

        sqlx::query!(
            r#"
            SELECT
                message_id AS "message_id: MessageId",
                flow_id AS "flow_id: FlowId",
                kind AS "kind: MessageKind",
                timestamp AS "timestamp: DateTime<FixedOffset>",
                data as "data: MessageData",
                metadata AS "metadata: Metadata"
            FROM message
            WHERE
                (flow_id = ?1 OR ?1 IS NULL)
                AND
                (?2 > timestamp OR ?2 IS NULL)
                AND
                (timestamp < ?3 OR ?3 IS NULL)
            ORDER BY timestamp ASC
            LIMIT ?4
            "#,
            flow_id,
            after,
            before,
            limit,
        )
        .fetch_all(self.transaction.as_mut())
        .await?
        .into_iter()
        .map(|row| {
            Ok(Message {
                message_id: row.message_id,
                flow_id: row.flow_id,
                kind: row.kind,
                timestamp: row.timestamp,
                data: row.data,
                metadata: row.metadata,
            })
        })
        .collect::<Result<Vec<Message>, Error>>()
    }
}
