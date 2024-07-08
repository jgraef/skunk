use std::{
    collections::HashMap,
    sync::{
        Arc,
        RwLock,
    },
};

use skunk_api_protocol::{
    error::NoSuchSocket,
    socket::SocketId,
};
use skunk_util::trigger;
use uuid::Uuid;

use super::socket;

#[derive(Clone, Debug)]
pub struct Context {
    inner: Arc<RwLock<Inner>>,
}

#[derive(Debug)]
struct Inner {
    sockets: HashMap<SocketId, socket::Sender>,
    reload_ui: trigger::Receiver,
}

impl Context {
    pub fn new(reload_ui: trigger::Receiver) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                sockets: HashMap::new(),
                reload_ui,
            })),
        }
    }

    pub fn connect_socket(&self, sender: socket::Sender) -> SocketId {
        let id = SocketId::from(Uuid::new_v4());
        let mut inner = self.inner.write().unwrap();
        inner.sockets.insert(id, sender);
        id
    }

    pub fn socket(&self, id: SocketId) -> Result<socket::Sender, NoSuchSocket> {
        let inner = self.inner.read().unwrap();

        let socket = inner
            .sockets
            .get(&id)
            .cloned()
            .ok_or_else(|| NoSuchSocket { id })?;

        // check if socket is closed. if it is, remove from hashmap
        if socket.is_closed() {
            // race-condition here doesn't matter. don't care if another thread removes the
            // socket first.
            drop(inner);
            let mut inner = self.inner.write().unwrap();
            inner.sockets.remove(&id);
            return Err(NoSuchSocket { id });
        }

        Ok(socket)
    }

    pub fn reload_ui(&self) -> trigger::Receiver {
        let inner = self.inner.read().unwrap();
        inner.reload_ui.clone()
    }
}
