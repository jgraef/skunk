#![allow(dead_code)]

mod app;
mod core;
mod util;

use color_eyre::eyre::Error;
use structopt::StructOpt;

use crate::app::{
    App,
    Args,
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenvy::dotenv().ok();
    color_eyre::install()?;
    tracing_subscriber::fmt::init();

    let args = Args::from_args();
    App::new(args.options)?.run(args.command).await?;

    Ok(())
}
