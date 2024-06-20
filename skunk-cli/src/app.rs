use std::{
    collections::HashSet,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
};

use byst::{
    hexdump::hexdump,
    io::read,
    Bytes,
};
use color_eyre::eyre::{
    bail,
    Error,
};
use skunk::{
    address::TcpAddress,
    protocol::{
        http,
        inet::ethernet,
        tls,
    },
    proxy::{
        fn_proxy,
        pcap::{
            self,
            interface::Interface,
            socket::Mode,
        },
        socks::server as socks,
        DestinationAddress,
        Passthrough,
        Proxy,
    },
    util::CancellationToken,
};
use structopt::StructOpt;
use tokio::{
    net::TcpStream,
    task::JoinSet,
};

use crate::config::Config;

/// skunk - 🦨 A person-in-the-middle proxy
#[derive(Debug, StructOpt)]
pub enum Command {
    /// Generates key and root certificate for the certificate authority used to
    /// intercept TLS traffic.
    Ca {
        /// Overwrite existing files.
        #[structopt(short, long)]
        force: bool,
    },
    /// Example command to log (possibly decrypted) HTTP traffic to console.
    LogHttp {
        #[structopt(flatten)]
        socks: SocksArgs,

        #[structopt(flatten)]
        pcap: PcapArgs,

        /// Target host:port addresses.
        ///
        /// This can be used to only selectively inspect traffic. By default all
        /// traffic is inspected. Currently only ports 80 and 443 are supported.
        target: Vec<TcpAddress>,
    },
    Proxy {
        #[structopt(flatten)]
        socks: SocksArgs,

        #[structopt(short, long)]
        rule: Vec<PathBuf>,
    },
}

#[derive(Debug, StructOpt)]
pub struct SocksArgs {
    /// Enable socks proxy
    #[structopt(name = "socks", long = "socks")]
    enabled: bool,

    /// Bind address for the SOCKS proxy.
    #[structopt(long = "socks-bind-address", default_value = "127.0.0.1:9090")]
    bind_address: SocketAddr,

    /// Username for the SOCKS proxy. If this is specified, --socks-password
    /// needs to be specified as well.
    #[structopt(long = "socks-username")]
    username: Option<String>,

    /// Password for the SOCKS proxy. If this is specified, --socks-username
    /// needs to be specified as well.
    #[structopt(long = "socks-password")]
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
pub struct PcapArgs {
    #[structopt(name = "pcap", long = "pcap")]
    enabled: bool,

    #[structopt(long = "pcap-interface")]
    interface: Option<String>,

    #[structopt(long = "pcap-ap")]
    ap: bool,
}

/// Skunk app command-line options (i.e. command-line arguments without the
/// actual command to run).
#[derive(Debug, StructOpt)]
pub struct Options {
    /// Path to the skunk configuration directory. Defaults to
    /// `~/.config/gocksec/skunk/`.
    #[structopt(short, long, env = "SKUNK_CONFIG")]
    config: Option<PathBuf>,
}

/// Skunk app command-line arguments.
#[derive(Debug, StructOpt)]
pub struct Args {
    /// General options for the skunk command-line.
    #[structopt(flatten)]
    pub options: Options,

    /// The specific command to run.
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

    /// Runs the given command-line command.
    pub async fn run(&mut self, command: Command) -> Result<(), Error> {
        match command {
            Command::Ca { force } => {
                self.generate_ca(force).await?;
            }
            Command::LogHttp {
                socks,
                pcap,
                target,
            } => {
                self.log_http(socks, pcap, target).await?;
            }
            Command::Proxy { socks, rule } => {
                self.proxy(socks, rule).await?;
            }
        }

        Ok(())
    }

    /// Generates the CA.
    async fn generate_ca(&self, force: bool) -> Result<(), Error> {
        let ca = tls::Ca::generate().await?;
        let key_file = self.config.relative_path(&self.config.ca.key_file);
        let cert_file = self.config.relative_path(&self.config.ca.cert_file);

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

    /// Example command to log (possibly decrypted) HTTP traffic to console.
    async fn log_http(
        &self,
        socks: SocksArgs,
        pcap: PcapArgs,
        target: Vec<TcpAddress>,
    ) -> Result<(), Error> {
        let pcap_interface = if pcap.enabled {
            fn print_interfaces() -> Result<(), Error> {
                println!("available interfaces:");
                for interface in Interface::list()? {
                    println!("{interface:#?}\n");
                }
                Ok(())
            }

            if let Some(interface) = pcap.interface {
                let interface_opt = Interface::from_name(&interface);
                if interface_opt.is_none() {
                    eprintln!("interface '{interface}' not found");
                    print_interfaces()?;
                    return Ok(());
                }
                interface_opt
            }
            else {
                print_interfaces()?;
                return Ok(());
            }
        }
        else {
            None
        };

        // open CA
        let ca = tls::Ca::open(
            self.config.path.join(&self.config.config.ca.key_file),
            self.config.path.join(&self.config.config.ca.cert_file),
        )?;

        // create TLS context
        let tls = tls::Context::new(ca).await?;

        // target filters
        let filter = Arc::new(if target.is_empty() {
            tracing::info!("matching all flows");
            TargetFilter::All
        }
        else {
            tracing::info!("matching: {target:?}");
            TargetFilter::Set(target.into_iter().collect())
        });

        // debug: for debugging it's more convenient to kill the process on Ctrl-C
        //let shutdown = CancellationToken::default();
        let shutdown = cancel_on_ctrlc_or_sigterm();

        let mut join_set = JoinSet::new();

        if socks.enabled {
            let shutdown = shutdown.clone();
            join_set.spawn(async move {
                // run the SOCKS server. `proxy` will handle connections. The default
                // [`Connect`][skunk::connect::Connect] (i.e.
                // [`ConnectTcp`][skunk::connect::ConnectTcp]) is used.
                socks
                    .builder()?
                    .with_graceful_shutdown(shutdown)
                    .with_proxy(fn_proxy(move |incoming, outgoing| {
                        proxy(tls.clone(), filter.clone(), incoming, outgoing)
                    }))
                    .serve()
                    .await?;
                Ok::<(), Error>(())
            });
        }

        if let Some(interface) = pcap_interface {
            join_set.spawn({
                let shutdown = shutdown.clone();
                let interface = interface.clone();
                async move {
                    if pcap.enabled {
                        let country_code = std::env::var("HOSTAPD_CC")
                        .expect("Environment variable `HOSTAPD_CC` not set. You need to set this variable to your country code.");

                        tracing::info!("starting hostapd");
                        let mut hostapd = pcap::ap::Builder::new(&interface, &country_code)
                                .with_channel(11)
                                .with_graceful_shutdown(shutdown.clone())
                                .start()?;

                        tracing::info!("waiting for hostapd to configure the interface...");
                        hostapd.ready().await?;
                        tracing::info!("hostapd ready");
                    }

                    pcap_run(interface, shutdown).await?;
                    Ok::<(), Error>(())
                }
            });

            if pcap.enabled {
                join_set.spawn({
                    let _shutdown = shutdown.clone();
                    let _interface = interface.clone();
                    async move {
                        //pcap::dhcp::run(&interface, shutdown, Default::default()).await?;
                        Ok::<(), Error>(())
                    }
                });
            }
        }

        while let Some(()) = join_set.join_next().await.transpose()?.transpose()? {}

        Ok(())
    }

    async fn proxy(&self, _socks: SocksArgs, _rules: Vec<PathBuf>) -> Result<(), Error> {
        todo!();
    }
}

/// Proxy connections.
///
/// This will first check if that connection matches any filters. Then it will
/// decide using the port whether to decrypt TLS for that connection. Finally it
/// will run a HTTP server and client to proxy HTTP requests.
async fn proxy(
    tls: tls::Context,
    filter: Arc<TargetFilter>,
    incoming: socks::Incoming,
    outgoing: TcpStream,
) -> Result<(), skunk::Error> {
    let destination_address = incoming.destination_address();

    if filter.matches(destination_address) {
        let span = tracing::info_span!("connection", destination = %destination_address);

        let is_tls = destination_address.port == 443;
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

/// A simple filter to decide which target addresses should be intercepted.
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

/// Returns a [`CancellationToken`] that will be triggered when Ctrl-C is
/// pressed, or (on Unix) when SIGTERM is received.
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

async fn pcap_run(interface: Interface, shutdown: CancellationToken) -> Result<(), Error> {
    async fn handle_packet(mut packet: Bytes) -> Result<(), Error> {
        println!("{}", hexdump(&packet));

        let frame = read!(&mut packet => ethernet::Packet)?;

        tracing::debug!(?frame);

        Ok(())
    }

    let (_sender, mut receiver) = interface.channel(Mode::Raw)?;

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            result = receiver.receive::<Bytes>() => {
                let packet = result?;
                handle_packet(packet).await?;
            }
        }
        todo!();
    }

    Ok(())
}
