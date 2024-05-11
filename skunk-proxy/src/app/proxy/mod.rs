pub mod http;
pub mod socks;

use std::{
    future::Future,
    path::PathBuf,
    pin::Pin,
};

use structopt::StructOpt;

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
