use std::{
    collections::BTreeMap,
    net::SocketAddr,
};

use axum::Router;
use serde::{
    Deserialize,
    Serialize,
};

use super::Context;

pub(crate) fn router() -> Router<Context> {
    todo!();
}

#[derive(Debug)]
pub struct Captures {
    inner: BTreeMap<Id, ()>,
}

impl Captures {}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Id {
    SocksProxy { bind_address: SocketAddr },
    HttpProxy { bind_address: SocketAddr },
    PacketCapture { interface: String },
}
