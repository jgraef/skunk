#![allow(dead_code)]

mod app;
mod config;

use color_eyre::eyre::Error;
use structopt::StructOpt;
use tracing_subscriber::EnvFilter;

use crate::app::{
    App,
    Args,
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenvy::dotenv().ok();
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .pretty()
        .init();

    let args = Args::from_args();
    App::new(args.options)?.run(args.command).await?;

    Ok(())
}
