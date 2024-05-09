mod socks;
mod http;

use std::{
    future::Future, net::SocketAddr, path::PathBuf, pin::Pin
};

use structopt::StructOpt;
use tokio::io::{AsyncRead, AsyncWrite};
use async_trait::async_trait;

use super::{
    store::{
        Location,
        Store,
    },
    Error,
};

#[derive(Debug, StructOpt)]
pub struct Args {
    /// File to store intercepted requests and responses in.
    file: Option<PathBuf>,

    /// Name for this session.
    #[structopt(short, long = "name")]
    session_name: Option<String>,
}

pub struct Proxy {
    store: Store,
    shutdown_signal: Option<Pin<Box<dyn Future<Output = ()>>>>,
}

impl Proxy {
    pub async fn new(args: Args) -> Result<Self, Error> {
        let store = Store::open(Location::from_option(args.file.as_deref())).await?;

        // on startup/request: create CA
        // create http proxy
        // create session (only one at a time)
        //   - proxy then intercepts

        Ok(Self {
            store,
            shutdown_signal: None,
        })
    }

    pub fn with_graceful_shutdown(mut self, signal: impl Future<Output = ()> + 'static) -> Self {
        self.shutdown_signal = Some(Box::pin(signal));
        self
    }

    pub async fn run(self) -> Result<(), Error> {
        todo!();
    }
}

#[derive(Clone, Debug)]
pub enum Address {
    SocketAddress(SocketAddr),
    DomainAddress {
        hostname: String,
        port: u16,
    }
}

#[async_trait]
pub trait Connect: Clone + Send + 'static {
    type Connection: AsyncRead + AsyncWrite + Send;

    async fn connect(&self, address: Address) -> Result<Self::Connection, Error>;
}