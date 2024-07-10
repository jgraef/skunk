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
    error::{
        ApiError,
        NoSuchSocket,
    },
    flow::{
        Event,
        Flow,
        FlowId,
        GetFlowsRequest,
        GetFlowsResponse,
        Message,
        Subscribe,
    },
    socket::{
        ServerMessage,
        SocketId,
        SubscriptionId,
    },
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

    pub async fn unsubscribe(&self, socket_id: SocketId, subscription_id: SubscriptionId) {
        let mut subscriptions = self.subscriptions.write().await;
        subscriptions.remove(socket_id, subscription_id);
    }

    pub async fn begin_flow(&self, flow: &Flow) -> Result<(), Error> {
        // todo: is this the right ordering, in regards to get_flows?

        let mut transaction = self.flow_store.transaction().await?;
        transaction.insert_flow(&flow).await?;
        let mut subscriptions = self.subscriptions.write().await;
        transaction.commit().await?;
        subscriptions.begin_flow(&flow).await?;

        Ok(())
    }

    pub async fn emit_message(&self, message: Message) -> Result<(), Error> {
        let mut transaction = self.flow_store.transaction().await?;
        transaction.insert_message(&message).await?;
        let mut subscriptions = self.subscriptions.write().await;
        transaction.commit().await?;
        subscriptions.flow_message(&message).await?;

        Ok(())
    }

    pub async fn end_flow(&self, flow_id: FlowId) -> Result<(), Error> {
        let mut subscriptions = self.subscriptions.write().await;
        subscriptions.end_flow(flow_id).await?;
        Ok(())
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
                subscriptions.insert(sender, subscription_id);
            }
            _ => unreachable!(),
        }

        let flows = transaction.get_flows(parent, after, before, limit).await?;

        transaction.commit().await?;

        Ok(flows)
    }
}

#[derive(Debug, Default)]
pub struct Subscriptions {
    inner: HashMap<(SocketId, SubscriptionId), socket::Sender>,
}

impl Subscriptions {
    pub fn insert(&mut self, socket: socket::Sender, subscription_id: SubscriptionId) {
        self.inner
            .insert((socket.socket_id(), subscription_id), socket);
    }

    pub fn remove(&mut self, socket_id: SocketId, subscription_id: SubscriptionId) {
        self.inner.remove(&(socket_id, subscription_id));
    }

    async fn for_each(
        &mut self,
        mut f: impl FnMut(SubscriptionId) -> ServerMessage,
    ) -> Result<(), Error> {
        let mut remove = vec![];

        for ((socket_id, subscription_id), sender) in self.inner.iter_mut() {
            let message = f(*subscription_id);

            if let Err(socket::Closed) = sender.send_message(message).await {
                remove.push((*socket_id, *subscription_id));
            }
        }

        for key in remove {
            self.inner.remove(&key);
        }

        Ok(())
    }

    pub async fn begin_flow(&mut self, flow: &Flow) -> Result<(), Error> {
        self.for_each(|subscription_id| {
            ServerMessage::FlowEvent {
                subscription_id,
                event: Event::BeginFlow { flow: flow.clone() },
            }
        })
        .await
    }

    pub async fn end_flow(&mut self, flow_id: FlowId) -> Result<(), Error> {
        self.for_each(|subscription_id| {
            ServerMessage::FlowEvent {
                subscription_id,
                event: Event::EndFlow { flow_id },
            }
        })
        .await
    }

    pub async fn flow_message(&mut self, message: &Message) -> Result<(), Error> {
        self.for_each(|subscription_id| {
            ServerMessage::FlowEvent {
                subscription_id,
                event: Event::Message {
                    message: message.clone(),
                },
            }
        })
        .await
    }
}
