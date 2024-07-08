use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    socket::SocketId,
    util::{
        api_request,
        api_response,
    },
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetFlowsRequest {
    pub subscribe: Option<SocketId>,
    // todo
}

api_request!(GetFlowsRequest);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetFlowsResponse {
    // todo
}

api_response!(GetFlowsResponse);
