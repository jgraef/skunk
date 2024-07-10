use chrono::{
    DateTime,
    FixedOffset,
};
use indexmap::IndexMap;
use serde::{
    Deserialize,
    Serialize,
};
use uuid::Uuid;

use crate::{
    socket::{
        SocketId,
        SubscriptionId,
    },
    util::{
        api_request,
        api_response,
        sqlx_json_type,
    },
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetFlowsRequest {
    pub parent: Option<FlowId>,
    pub after: Option<DateTime<FixedOffset>>,
    pub before: Option<DateTime<FixedOffset>>,
    pub limit: Option<usize>,
    pub subscribe: Option<Subscribe>,
}

api_request!(GetFlowsRequest);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Subscribe {
    pub socket_id: SocketId,
    pub subscription_id: SubscriptionId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetFlowsResponse {
    pub flows: Vec<Flow>,
}

api_response!(GetFlowsResponse);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct FlowId(pub Uuid);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Flow {
    pub flow_id: FlowId,
    pub parent: Option<FlowId>,
    pub protocol: Option<String>,
    pub timestamp: DateTime<FixedOffset>,
    pub metadata: Metadata,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct Metadata {
    inner: IndexMap<String, Box<serde_json::value::RawValue>>,
}

sqlx_json_type!(Metadata);

impl Metadata {
    pub fn get<T: for<'de> Deserialize<'de>>(
        &self,
        key: &str,
    ) -> Result<Option<T>, serde_json::Error> {
        self.inner
            .get(key)
            .map(|value| T::deserialize(value.as_ref()))
            .transpose()
    }

    pub fn contains(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }

    pub fn insert<T: Serialize>(
        &mut self,
        key: String,
        value: &T,
    ) -> Result<(), serde_json::Error> {
        self.inner
            .insert(key, serde_json::value::to_raw_value(value)?);
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Event {
    BeginFlow { flow: Flow },
    EndFlow { flow_id: FlowId },
    Message { message: Message },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct MessageId(pub Uuid);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub message_id: MessageId,
    pub flow_id: FlowId,
    pub kind: MessageKind,
    pub timestamp: DateTime<FixedOffset>,
    pub data: MessageData,
    pub metadata: Metadata,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct MessageData {
    // todo: We should Arc this.
    inner: Box<serde_json::value::RawValue>,
}

impl MessageData {
    pub fn from_value<T: Serialize>(value: &T) -> Result<Self, serde_json::Error> {
        Ok(Self {
            inner: serde_json::value::to_raw_value(value)?,
        })
    }

    pub fn to_value<T: for<'de> Deserialize<'de>>(&self) -> Result<T, serde_json::Error> {
        T::deserialize(self.inner.as_ref())
    }
}

sqlx_json_type!(MessageData);

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum MessageKind {
    #[default]
    Other,
    Request,
    Response,
}
