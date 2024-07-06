use std::borrow::Cow;

pub use semver::Version;
use serde::{
    Deserialize,
    Serialize,
};

pub type Id = u32;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientHello {
    pub user_agent: Cow<'static, str>,
    pub app_version: Version,
    pub protocol_version: Version,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerHello {
    pub server_agent: Cow<'static, str>,
    pub app_version: Version,
    pub protocol_version: Version,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerMessage {
    HotReload,
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
