use std::sync::Arc;

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
    error::{
        ApiError,
        NoSuchSocket,
    },
    flow::{
        Flow,
        FlowId,
        GetFlowsRequest,
        GetFlowsResponse,
        Metadata,
        Subscribe,
    },
    socket::SubscriptionId,
};
use skunk_flows_store::FlowStore;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::{
    socket::{
        self,
        Subscriptions,
    },
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
        .map(
            |Subscribe {
                 socket_id,
                 subscription_id,
             }| {
                Ok::<_, NoSuchSocket>((context.socket(socket_id)?, subscription_id))
            },
        )
        .transpose()?;

    let flows = context
        .flows
        .get_flows(
            request.parent,
            request.after,
            request.before,
            request.limit,
            subscribe,
        )
        .await?;

    Ok(GetFlowsResponse { flows })
}

#[derive(Clone, Debug)]
pub struct Flows {
    flow_store: FlowStore,
    subscriptions: Arc<RwLock<Subscriptions>>,
}

impl Flows {
    pub fn new(flow_store: Option<FlowStore>) -> Self {
        let flow_store = flow_store.unwrap_or_else(|| todo!("Unopened FlowStore not implemented"));

        Self {
            flow_store,
            subscriptions: Arc::new(RwLock::new(Default::default())),
        }
    }

    pub async fn begin_flow(
        &self,
        parent: Option<FlowId>,
        protocol: Option<String>,
        timestamp: DateTime<FixedOffset>,
        metadata: Metadata,
    ) -> Result<FlowId, Error> {
        let flow_id = FlowId(Uuid::new_v4());

        let flow = Flow {
            flow_id,
            parent,
            protocol,
            timestamp,
            metadata,
        };

        // todo: is this the right ordering, in regards to get_flows?

        let mut transaction = self.flow_store.transaction().await?;

        transaction.create_flow(&flow).await?;

        let mut subscriptions = self.subscriptions.write().await;

        transaction.commit().await?;

        subscriptions.begin_flow(&flow).await?;

        Ok(flow_id)
    }

    pub async fn get_flows(
        &self,
        parent: Option<FlowId>,
        after: Option<DateTime<FixedOffset>>,
        before: Option<DateTime<FixedOffset>>,
        limit: Option<usize>,
        subscribe: Option<(socket::Sender, SubscriptionId)>,
    ) -> Result<Vec<Flow>, Error> {
        let subscriptions = if subscribe.is_some() {
            Some(self.subscriptions.write().await)
        }
        else {
            None
        };

        let mut transaction = self.flow_store.transaction().await?;

        match (subscriptions, subscribe) {
            (None, None) => {}
            (Some(mut subscriptions), Some((sender, subscription_id))) => {
                subscriptions.insert(subscription_id, sender);
            }
            _ => unreachable!(),
        }

        let flows = transaction.get_flows(parent, after, before, limit).await?;

        transaction.commit().await?;

        Ok(flows)
    }
}
