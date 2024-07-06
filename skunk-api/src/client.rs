#![allow(dead_code)]

use futures_util::{
    SinkExt,
    TryFutureExt,
    TryStreamExt,
};
use reqwest_websocket::{
    Message,
    RequestBuilderExt,
};
use serde::{
    Deserialize,
    Serialize,
};
use url::Url;

#[derive(Debug, thiserror::Error)]
#[error("API client error")]
pub enum Error {
    Reqwest(#[from] reqwest::Error),
    Websocket(#[from] reqwest_websocket::Error),
    Decode(#[from] rmp_serde::decode::Error),
    Encode(#[from] rmp_serde::encode::Error),
}

#[derive(Debug)]
pub struct Client {
    client: reqwest::Client,
    base_url: UrlBuilder,
}

impl Client {
    pub fn new(base_url: Url) -> Self {
        let client = reqwest::Client::new();
        let base_url = UrlBuilder { url: base_url };

        tokio::task::spawn_local({
            let client = client.clone();
            let base_url = base_url.clone();
            reactor(client, base_url).map_err(|e| {
                tracing::error!("{e}");
            })
        });

        Self { client, base_url }
    }
}

#[derive(Clone, Debug)]
struct UrlBuilder {
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

async fn reactor(client: reqwest::Client, base_url: UrlBuilder) -> Result<(), Error> {
    let mut websocket = WebSocket::new(
        client
            .get(base_url.push("ws").finish())
            .upgrade()
            .send()
            .await?
            .into_websocket()
            .await?,
    );

    while let Some(event) = websocket.receive::<Event>().await? {
        match event {
            // todo
        }
    }

    Ok(())
}

// todo: this needs to be in a crate that is shared between api client and
// server
#[derive(Clone, Debug, Deserialize)]
enum Event {
    // todo
}

// todo: this needs to be in a crate that is shared between api client and
// server
enum Command {
    // todo
}

/// Wrapper around [`reqwest_websocket::WebSocket`] that sends and receives
/// msgpack-encoded messages.
#[derive(Debug)]
struct WebSocket {
    inner: reqwest_websocket::WebSocket,
}

impl WebSocket {
    pub fn new(inner: reqwest_websocket::WebSocket) -> Self {
        Self { inner }
    }

    pub async fn receive<T: for<'de> Deserialize<'de>>(&mut self) -> Result<Option<T>, Error> {
        while let Some(message) = self.inner.try_next().await? {
            match message {
                Message::Binary(data) => {
                    let item: T = rmp_serde::from_slice(&data)?;
                    return Ok(Some(item));
                }
                _ => {}
            }
        }

        Ok(None)
    }

    pub async fn send<T: Serialize>(&mut self, item: &T) -> Result<(), Error> {
        let data = rmp_serde::to_vec(item)?;
        self.inner.send(Message::Binary(data)).await?;
        Ok(())
    }
}
