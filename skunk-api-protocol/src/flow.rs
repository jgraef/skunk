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
    inner: IndexMap<String, serde_json::Value>,
}

sqlx_json_type!(Metadata);
