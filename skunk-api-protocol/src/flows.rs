use std::collections::HashMap;

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
    socket::SocketId,
    util::{
        api_request,
        api_response,
    },
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetFlowsRequest {
    pub after: Option<DateTime<FixedOffset>>,
    pub before: Option<DateTime<FixedOffset>>,
    pub subscribe: Option<SocketId>,
}

api_request!(GetFlowsRequest);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetFlowsResponse {
    pub flows: Vec<Flow>,
}

api_response!(GetFlowsResponse);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlowId(pub Uuid);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProtocolId(pub Uuid);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Flow {
    pub flow_id: FlowId,
    pub destination_address: String,
    pub destination_port: u16,
    pub protocol: Option<ProtocolId>,
    pub timestamp: DateTime<FixedOffset>,
    pub metadata: Metadata,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct Metadata {
    inner: IndexMap<String, serde_json::Value>,
}
