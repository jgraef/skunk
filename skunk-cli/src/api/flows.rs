use axum::{
    extract::State,
    routing,
    Router,
};
use skunk_api_protocol::{
    error::ApiError,
    flows::{
        GetFlowsRequest,
        GetFlowsResponse,
    },
};
use skunk_flows_store::FlowStore;

use super::Context;

pub(super) fn router() -> Router<Context> {
    Router::new().route("/", routing::get(get_flows))
}

async fn get_flows(
    State(context): State<Context>,
    request: GetFlowsRequest,
) -> Result<GetFlowsResponse, ApiError> {
    let subscribe = request
        .subscribe
        .map(|socket_id| context.socket(socket_id))
        .transpose()?;

    todo!();
}

#[derive(Clone, Debug)]
pub struct Flows {
    store: FlowStore,
    //subscriptions:
}
