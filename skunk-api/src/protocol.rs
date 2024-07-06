use serde::{
    Deserialize,
    Serialize,
};

pub type Id = u32;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerMessage {
    // todo
    Interrupt {
        continue_tx: Id,
        // todo: request/response/etc.
    },
    Flow {
        // todo
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    SubscribeFlows,
    Start,
    Stop,
    Continue {
        continue_tx: Id,
        // todo: modified request/response/etc.
    },
}
