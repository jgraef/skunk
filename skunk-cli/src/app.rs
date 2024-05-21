use std::{
    collections::HashSet,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
};

use color_eyre::eyre::{
    bail,
    Error,
};
use skunk::{
    address::TcpAddress,
    protocol::{
        http,
        tls,
    },
    proxy::{
        fn_proxy,
        socks,
        Passthrough,
        Proxy,
        TargetAddress,
    },
    util::CancellationToken,
};
use structopt::StructOpt;
use tokio::net::TcpStream;

use crate::config::Config;

#[derive(Debug, StructOpt)]
pub enum Command {
    Ca {
        #[structopt(short, long)]
        force: bool,
    },
    LogHttp {
        #[structopt(flatten)]
        socks: SocksArgs,

        target: Vec<TcpAddress>,
    },
}

#[derive(Debug, StructOpt)]
pub struct SocksArgs {
    #[structopt(short, long, default_value = "127.0.0.1:9090")]
    bind_address: SocketAddr,

    #[structopt(long)]
    username: Option<String>,

    #[structopt(long)]
    password: Option<String>,
}

impl SocksArgs {
    pub fn builder(self) -> Result<socks::Builder, Error> {
        let mut builder = socks::Builder::default().with_bind_address(self.bind_address);

        match (self.username, self.password) {
            (Some(username), Some(password)) => builder = builder.with_password(username, password),
            (None, None) => {}
            _ => bail!("Either both username and password or neither must be specified"),
        }

        Ok(builder)
    }
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
            Command::Ca { force } => {
                self.generate_ca(force).await?;
            }
            Command::LogHttp { socks, target } => {
                self.log_http(socks, target).await?;
            }
        }

        Ok(())
    }

    async fn generate_ca(&self, force: bool) -> Result<(), Error> {
        let ca = tls::Ca::generate().await?;
        let key_file = self.config.path.join(&self.config.config.ca.key_file);
        let cert_file = self.config.path.join(&self.config.config.ca.cert_file);

        if !force {
            if key_file.exists() {
                tracing::error!(key_file = %key_file.display(), "Key file already exists. Aborting. Run with --force to overwrite existing files.");
                return Ok(());
            }
            if cert_file.exists() {
                tracing::error!(cert_file = %cert_file.display(), "Cert file already exists. Aborting. Run with --force to overwrite existing files.");
                return Ok(());
            }
        }

        ca.save(&key_file, &cert_file)?;

        tracing::info!(key_file = %key_file.display(), "Key file saved.");
        tracing::info!(cert_file = %cert_file.display(), "Cert file saved.");

        Ok(())
    }

    async fn log_http(&self, socks: SocksArgs, target: Vec<TcpAddress>) -> Result<(), Error> {
        let ca = tls::Ca::open(
            self.config.path.join(&self.config.config.ca.key_file),
            self.config.path.join(&self.config.config.ca.cert_file),
        )?;
        let tls = tls::Context::new(ca).await?;

        let filter = Arc::new(if target.is_empty() {
            tracing::info!("matching all flows");
            TargetFilter::All
        }
        else {
            tracing::info!("matching: {target:?}");
            TargetFilter::Set(target.into_iter().collect())
        });

        socks
            .builder()?
            .with_graceful_shutdown(cancel_on_ctrlc_or_sigterm())
            .with_proxy(fn_proxy(move |incoming, outgoing| {
                proxy(tls.clone(), filter.clone(), incoming, outgoing)
            }))
            .serve()
            .await?;

        Ok(())
    }
}

async fn proxy(
    tls: tls::Context,
    filter: Arc<TargetFilter>,
    incoming: socks::Incoming,
    outgoing: TcpStream,
) -> Result<(), skunk::Error> {
    let target_address = incoming.target_address();

    if filter.matches(target_address) {
        let span = tracing::info_span!("connection", target = %target_address);

        let is_tls = target_address.port == 443;
        let (incoming, outgoing) = tls.maybe_decrypt(incoming, outgoing, is_tls).await?;

        http::proxy(incoming, outgoing, |request, send_request| {
            let span = tracing::info_span!(
                parent: &span,
                "request",
                method = %request.method(),
                uri = %request.uri()
            );

            async move {
                // log request
                tracing::info!(
                    parent: &span,
                    ">"
                );

                let response = send_request.send(request).await?;

                // log response
                tracing::info!(
                    parent: &span,
                    status = %response.status(),
                    "<"
                );

                Ok(response)
            }
        })
        .await?;
    }
    else {
        Passthrough.proxy(incoming, outgoing).await?;
    };

    Ok::<_, skunk::Error>(())
}

#[derive(Clone, Debug)]
pub enum TargetFilter {
    All,
    Set(HashSet<TcpAddress>),
}

impl TargetFilter {
    pub fn matches(&self, address: &TcpAddress) -> bool {
        if address.port != 80 && address.port != 443 {
            tracing::info!(%address, "connection to ignored");
            return false;
        }

        match self {
            TargetFilter::All => true,
            TargetFilter::Set(targets) => targets.contains(address),
        }
    }
}

fn cancel_on_ctrlc_or_sigterm() -> CancellationToken {
    let token = CancellationToken::new();

    async fn sigterm() {
        #[cfg(unix)]
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .unwrap()
            .recv()
            .await;

        #[cfg(not(unix))]
        futures::future::pending().await;
    }

    tokio::spawn({
        let token = token.clone();
        async move {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("Received Ctrl-C. Shutting down.");
                }
                _ = sigterm() => {
                    tracing::info!("Received SIGTERM. Shutting down.");
                }
            }

            token.cancel();
        }
    });

    token
}
