use std::borrow::Cow;

pub use semver::Version;
use serde::{
    Deserialize,
    Serialize,
};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SocketId(Uuid);

impl From<Uuid> for SocketId {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

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
    pub socket_id: SocketId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerMessage {
    HotReload,
    Pong,
    // todo
    Interrupt {
        message_id: Uuid,
        // todo: request/response/etc.
    },
    Flow {
        // todo
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    Ping,
    SubscribeFlows,
    Start,
    Stop,
    Continue {
        message_id: Uuid,
        // todo: modified request/response/etc.
    },
}
