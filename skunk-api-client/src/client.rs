#![allow(dead_code)]

use std::{
    fmt::Debug,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use futures_util::{
    Future,
    FutureExt,
};
use skunk_util::trigger;
use tokio::sync::watch;
use tracing::Instrument;
use url::Url;

use crate::{
    socket::{
        Command,
        Reactor,
        ReactorHandle,
    },
    Status,
};

#[derive(Clone, Debug)]
pub struct Client {
    client: reqwest::Client,
    base_url: UrlBuilder,
    reactor: ReactorHandle,
}

impl Client {
    pub fn new(base_url: Url) -> (Self, Connection) {
        let client = reqwest::Client::new();
        let base_url = UrlBuilder { url: base_url };

        let (reactor, reactor_handle) =
            Reactor::new(client.clone(), base_url.clone().push("ws").finish());
        let span = tracing::info_span!("socket");

        let connection = Connection {
            inner: Box::pin(reactor.run().instrument(span)),
        };

        let client = Self {
            client,
            base_url,
            reactor: reactor_handle,
        };

        (client, connection)
    }

    async fn send_command(&mut self, command: Command) {
        self.reactor
            .command_tx
            .send(command)
            .await
            .expect("Reactor died");
    }

    pub fn reload_ui(&self) -> trigger::Receiver {
        self.reactor.reload_rx.clone()
    }

    pub fn status(&self) -> watch::Receiver<Status> {
        self.reactor.status_rx.clone()
    }

    pub async fn ping(&mut self) {
        let mut pong_rx = self.reactor.pong_rx.clone();
        self.send_command(Command::Ping).await;
        pong_rx.triggered().await;
    }
}

#[derive(Clone, Debug)]
pub(crate) struct UrlBuilder {
    url: Url,
}

impl UrlBuilder {
    pub fn push(mut self, s: impl ToString) -> Self {
        self.url.path_segments_mut().unwrap().push(&s.to_string());
        self
    }

    pub fn finish(self) -> Url {
        self.url
    }
}

/// Client connection.
///
/// This must be polled to drive the connection for a [`Client`].
pub struct Connection {
    inner: Pin<Box<dyn Future<Output = ()>>>,
}

impl Future for Connection {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.inner.poll_unpin(cx)
    }
}

impl Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection").finish_non_exhaustive()
    }
}
