use std::{
    collections::HashMap,
    sync::Arc,
};

use axum::{
    extract::{
        Query,
        State,
    },
    routing,
    Router,
};
use chrono::{
    DateTime,
    FixedOffset,
};
use skunk_api_protocol::{
    error::ApiError,
    flows::{
        Flow,
        GetFlowsRequest,
        GetFlowsResponse,
    },
    socket::SocketId,
};
use skunk_flows_store::FlowStore;
use tokio::sync::RwLock;

use super::{
    socket,
    Context,
    Error,
};

pub(super) fn router() -> Router<Context> {
    Router::new().route("/", routing::get(get_flows))
}

async fn get_flows(
    State(context): State<Context>,
    Query(request): Query<GetFlowsRequest>,
) -> Result<GetFlowsResponse, ApiError> {
    let subscribe = request
        .subscribe
        .map(|socket_id| context.socket(socket_id))
        .transpose()?;

    let flows = context
        .flows
        .get_flows(request.after, request.before, request.limit, subscribe)
        .await?;

    Ok(GetFlowsResponse { flows })
}

#[derive(Clone, Debug)]
pub struct Flows {
    flow_store: Option<FlowStore>,
    subscriptions: Arc<RwLock<HashMap<SocketId, socket::Sender>>>,
}

impl Flows {
    pub fn new(flow_store: Option<FlowStore>) -> Self {
        Self {
            flow_store,
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get_flows(
        &self,
        after: Option<DateTime<FixedOffset>>,
        before: Option<DateTime<FixedOffset>>,
        limit: Option<usize>,
        subscribe: Option<socket::Sender>,
    ) -> Result<Vec<Flow>, Error> {
        let subscriptions = if subscribe.is_some() {
            Some(self.subscriptions.write().await)
        }
        else {
            None
        };

        let transaction = if let Some(flow_store) = &self.flow_store {
            Some(flow_store.transaction().await?)
        }
        else {
            None
        };

        match (subscriptions, subscribe) {
            (None, None) => {}
            (Some(mut subscriptions), Some(subscribe)) => {
                subscriptions.insert(subscribe.socket_id(), subscribe);
            }
            _ => unreachable!(),
        }

        let flows = if let Some(mut transaction) = transaction {
            transaction.get_flows(after, before, limit).await?
        }
        else {
            Default::default()
        };

        Ok(flows)
    }
}
