mod ca;
mod config;
mod proxy;
mod store;

use std::path::PathBuf;

use structopt::StructOpt;

use self::{
    ca::Ca,
    config::Config,
    proxy::Proxy,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("config error")]
    Config(#[from] self::config::Error),

    #[error("store error")]
    Store(#[from] self::store::Error),

    #[error("ca error")]
    Ca(#[from] self::ca::Error),
}

#[derive(Debug, StructOpt)]
pub enum Command {
    Ca,
    Proxy(self::proxy::Args),
}

#[derive(Debug, StructOpt)]
pub struct Options {
    #[structopt(short, long, env = "SKUNK_CONFIG")]
    config: Option<PathBuf>,
}

#[derive(Debug, StructOpt)]
pub struct Args {
    #[structopt(flatten)]
    pub options: Options,

    #[structopt(subcommand)]
    pub command: Command,
}

pub struct App {
    options: Options,
    config: Config,
}

impl App {
    pub fn new(options: Options) -> Result<Self, Error> {
        let config = Config::open(options.config.as_ref())?;
        Ok(Self { options, config })
    }

    pub async fn run(self, command: Command) -> Result<(), Error> {
        match command {
            Command::Ca => {
                Ca::generate(&self.config).await?;
            }
            Command::Proxy(args) => {
                Proxy::new(args)
                    .await?
                    .with_graceful_shutdown(async {
                        let _ = tokio::signal::ctrl_c().await;
                    })
                    .run()
                    .await?;
            }
        }

        Ok(())
    }
}
