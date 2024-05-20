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

use crate::config::Config;

#[derive(Debug, StructOpt)]
pub enum Command {
    Ca,
    Proxy {
        /// File to store intercepted requests and responses in.
        //file: Option<PathBuf>,

        /// Name for this session.
        //#[structopt(short, long = "name")]
        //session_name: Option<String>,

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
            Command::Ca => {
                // todo: if file exists, ask if we want to replace them
                let ca = tls::Ca::generate().await?;
                let key_file = self.config.path.join(&self.config.config.ca.key_file);
                let cert_file = self.config.path.join(&self.config.config.ca.cert_file);
                ca.save(&key_file, &cert_file)?;
                tracing::info!("Key file saved to: {}", key_file.display());
                tracing::info!("Cert file saved to: {}", cert_file.display());
            }
            Command::Proxy { socks, target } => {
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
                    .with_proxy(fn_proxy(move |incoming: socks::Incoming, outgoing| {
                        let tls = tls.clone();
                        let filter = filter.clone();
                        async move {
                            let target_address = incoming.target_address();

                            if filter.matches(target_address) {
                                let span =
                                    tracing::info_span!("connection", target = %target_address);

                                let is_tls = target_address.port == 443;
                                let (incoming, outgoing) =
                                    tls.maybe_decrypt(incoming, outgoing, is_tls).await?;

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
                    }))
                    .serve()
                    .await?;
            }
        }

        Ok(())
    }
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
