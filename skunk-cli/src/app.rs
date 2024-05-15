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
    layer::{
        http::{
            self,
            Http,
        },
        tls::{
            self,
            Ca,
        },
        Layer,
        Passthrough,
    },
    proxy::{
        socks::{
            self,
            SocksSource,
        },
        ProxySource,
    },
    util::CancellationToken,
};
use structopt::StructOpt;
use tokio::net::TcpStream;
use tracing::Span;

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
    pub fn builder(self, shutdown: CancellationToken) -> Result<socks::Builder, Error> {
        let mut builder = socks::Builder::default()
            .with_bind_address(self.bind_address)
            .with_graceful_shutdown(shutdown);

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
                let ca = Ca::generate().await?;
                let key_file = self.config.path.join(&self.config.config.ca.key_file);
                let cert_file = self.config.path.join(&self.config.config.ca.cert_file);
                ca.save(&key_file, &cert_file)?;
                tracing::info!("Key file saved to: {}", key_file.display());
                tracing::info!("Cert file saved to: {}", cert_file.display());
            }
            Command::Proxy { socks, target } => {
                let shutdown = CancellationToken::new();

                // fixme: fn_layer doesn't work
                //let layer = fn_layer(|source: &mut SocksSource, target| async move {
                //    let target_address = source.target_address();
                //    Ok(())
                //});

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
                    .builder(shutdown)?
                    .with_layer(FilteredHttpsLayer { tls, filter })
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

#[derive(Clone, Debug)]
pub struct FilteredHttpsLayer {
    tls: tls::Context,
    filter: Arc<TargetFilter>,
}

impl<'source, 'target> Layer<&'source mut SocksSource, &'target mut TcpStream>
    for FilteredHttpsLayer
{
    type Output = ();

    async fn layer(
        &self,
        source: &'source mut SocksSource,
        target: &'target mut TcpStream,
    ) -> Result<(), skunk::Error> {
        let target_address = source.target_address();

        if self.filter.matches(target_address) {
            let span = tracing::info_span!("connection", target = %target_address);

            match target_address.port {
                80 => Http::new(LogLayer::new(span)).layer(source, target).await?,
                443 => {
                    tls::Tls::new(Http::new(LogLayer::new(span)), self.tls.clone())
                        .layer(source, target)
                        .await?
                }
                _ => panic!("only port 80 and 443 work right now"),
            }
        }
        else {
            Passthrough.layer(source, target).await?;
        };

        Ok(())
    }
}

#[derive(Debug)]
pub struct LogLayer {
    span: Span,
}

impl LogLayer {
    pub fn new(span: Span) -> Self {
        Self { span }
    }
}

impl<'client> Layer<http::Request, http::TargetClient<'client>> for LogLayer {
    type Output = http::Response;

    async fn layer(
        &self,
        request: http::Request,
        mut client: http::TargetClient<'client>,
    ) -> Result<http::Response, skunk::Error> {
        // log request
        tracing::info!(
            parent: &self.span,
            method = %request.0.method(),
            uri = %request.0.uri(),
            ">"
        );

        let response = client.send(request).await?;

        // log response
        tracing::info!(
            parent: &self.span,
            status = %response.0.status(),
            "<"
        );

        Ok(response)
    }
}
