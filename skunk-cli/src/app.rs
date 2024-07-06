use std::{
    collections::HashSet,
    sync::Arc,
};

use axum::Router;
use color_eyre::eyre::Error;
use skunk::{
    address::TcpAddress,
    protocol::{
        http,
        tls,
    },
    proxy::{
        fn_proxy,
        pcap::{
            self,
            interface::Interface,
            VirtualNetwork,
        },
        socks::server as socks,
        DestinationAddress,
        Passthrough,
        Proxy,
    },
    util::CancellationToken,
};
use tokio::{
    net::TcpStream,
    task::JoinSet,
};

use crate::{
    args::{
        Command,
        Options,
        ProxyArgs,
    },
    config::Config,
    serve_ui::ServeUi,
};

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
            Command::Proxy(args) => {
                self.proxy(args).await?;
            }
        }

        Ok(())
    }

    /// Generates the CA.
    async fn generate_ca(&self, force: bool) -> Result<(), Error> {
        let ca = tls::Ca::generate().await?;
        let key_file = self.config.config_relative_path(&self.config.tls.key_file);
        let cert_file = self.config.config_relative_path(&self.config.tls.cert_file);

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
    async fn proxy(&self, args: ProxyArgs) -> Result<(), Error> {
        let pcap_interface = if args.pcap.enabled {
            fn print_interfaces() -> Result<(), Error> {
                println!("available interfaces:");
                for interface in Interface::list()? {
                    println!("{interface:#?}\n");
                }
                Ok(())
            }

            if let Some(interface) = args.pcap.interface {
                let interface_opt = Interface::from_name(&interface)?;
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

        // create TLS context
        let tls = self.tls_context().await?;

        // target filters
        let filter = Arc::new(if args.targets.is_empty() {
            tracing::info!("matching all flows");
            TargetFilter::All
        }
        else {
            tracing::info!("matching: {:?}", args.targets);
            TargetFilter::Set(args.targets.into_iter().collect())
        });

        // shutdown token
        let shutdown = if args.no_graceful_shutdown {
            CancellationToken::default()
        }
        else {
            cancel_on_ctrlc_or_sigterm()
        };

        let mut join_set = JoinSet::new();

        if args.socks.enabled {
            let shutdown = shutdown.clone();
            join_set.spawn(async move {
                // run the SOCKS server. `proxy` will handle connections. The default
                // [`Connect`][skunk::connect::Connect] (i.e.
                // [`ConnectTcp`][skunk::connect::ConnectTcp]) is used.
                args.socks
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
                    if args.pcap.enabled {
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

                    let _network = VirtualNetwork::new(&interface)?;
                    shutdown.cancelled().await;
                    Ok::<(), Error>(())
                }
            });
        }

        if args.api.enabled {
            let shutdown = shutdown.clone();
            let mut api = skunk_api::server::builder();
            let serve_ui = ServeUi::from_config(&self.config, &mut api);

            join_set.spawn(async move {
                tracing::info!(bind_address = ?args.api.bind_address, "Starting API");

                let router = Router::new()
                    .nest("/api", api.finish())
                    .fallback_service(serve_ui);

                let listener = tokio::net::TcpListener::bind(args.api.bind_address).await?;
                axum::serve(listener, router)
                    .with_graceful_shutdown(shutdown.cancelled_owned())
                    .await?;

                Ok::<(), Error>(())
            });
        }

        // join all tasks
        while let Some(()) = join_set.join_next().await.transpose()?.transpose()? {}

        Ok(())
    }

    async fn tls_context(&self) -> Result<tls::Context, Error> {
        let ca = tls::Ca::open(
            self.config
                .config_relative_path(&self.config.config.tls.key_file),
            self.config
                .config_relative_path(&self.config.config.tls.cert_file),
        )?;
        Ok(tls::Context::new(ca).await?)
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
