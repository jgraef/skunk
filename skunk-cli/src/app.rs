use std::path::PathBuf;

use color_eyre::eyre::Error;
use skunk::tls::Ca;
use structopt::StructOpt;

use crate::config::Config;

#[derive(Debug, StructOpt)]
pub enum Command {
    Ca,
    Proxy {
        /// File to store intercepted requests and responses in.
        file: Option<PathBuf>,

        /// Name for this session.
        #[structopt(short, long = "name")]
        session_name: Option<String>,
    },
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

    pub async fn run(&mut self, command: Command) -> Result<(), Error> {
        match command {
            Command::Ca => {
                // todo: if file exists, ask if we want to replace them
                let ca = Ca::generate().await?;
                ca.save(
                    &self.config.config.ca.key_file,
                    &self.config.config.ca.cert_file,
                )?;
            }
            Command::Proxy { .. } => {
                todo!();
            }
        }

        Ok(())
    }
}
