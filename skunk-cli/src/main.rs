#![allow(dead_code)]

mod app;
mod args;
mod config;
mod serve_ui;

use clap::Parser;
use color_eyre::eyre::Error;
use tracing_subscriber::EnvFilter;

use crate::{
    app::App,
    args::Args,
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenvy::dotenv().ok();
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .pretty()
        .init();

    let args = Args::parse();
    App::new(args.options)?.run(args.command).await?;

    Ok(())
}
