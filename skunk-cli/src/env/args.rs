use std::{
    net::SocketAddr,
    path::PathBuf,
};

use clap::{
    builder::{
        styling::{
            AnsiColor,
            Color,
            Style,
        },
        Styles,
    },
    Parser,
};
use color_eyre::eyre::{
    bail,
    Error,
};
use skunk::{
    self,
    address::TcpAddress,
    proxy::socks::server as socks,
};

/// skunk - 🦨 A person-in-the-middle proxy
#[derive(Debug, Parser)]
#[clap(styles(Args::STYLES))]
pub struct Args {
    /// General options for the skunk command-line.
    #[clap(flatten)]
    pub options: Options,

    /// The specific command to run.
    #[clap(subcommand)]
    pub command: Command,
}

impl Args {
    const STYLES: Styles = Styles::styled()
        .header(Style::new().bold())
        .usage(Style::new().bold())
        .literal(
            Style::new()
                .italic()
                .fg_color(Some(Color::Ansi(AnsiColor::Magenta))),
        )
        .placeholder(
            Style::new()
                .italic()
                .fg_color(Some(Color::Ansi(AnsiColor::BrightGreen))),
        )
        .valid(Style::new().italic())
        .invalid(Style::new().fg_color(Some(Color::Ansi(AnsiColor::Red))));
}

#[derive(Debug, Parser)]
pub enum Command {
    /// Generates key and root certificate for the certificate authority used to
    /// intercept TLS traffic.
    GenerateCert {
        /// Overwrite existing files.
        #[clap(short, long)]
        force: bool,
    },
    /// Example command to log (possibly decrypted) HTTP traffic to console.
    Proxy(ProxyArgs),
}

#[derive(Debug, Parser)]
pub struct ProxyArgs {
    #[clap(flatten)]
    pub socks: SocksArgs,

    #[clap(flatten)]
    pub pcap: PcapArgs,

    #[clap(flatten)]
    pub api: ApiArgs,

    #[clap(long)]
    pub no_graceful_shutdown: bool,

    /// Target host:port addresses.
    ///
    /// This can be used to only selectively inspect traffic. By default all
    /// traffic is inspected. Currently only ports 80 and 443 are supported.
    pub targets: Vec<TcpAddress>,
}

#[derive(Debug, Parser)]
pub struct SocksArgs {
    /// Enable socks proxy
    #[clap(id = "socks_enabled", name = "socks", long = "socks")]
    pub enabled: bool,

    /// Bind address for the SOCKS proxy.
    #[clap(
        id = "socks_bind_address",
        value_name("ADDRESS"),
        long = "socks-bind-address",
        default_value = "127.0.0.1:9090"
    )]
    pub bind_address: SocketAddr,

    /// Username for the SOCKS proxy. If this is specified, --socks-password
    /// needs to be specified as well.
    #[clap(id = "socks_username", value_name("PASSWORD"), long = "socks-username")]
    pub username: Option<String>,

    /// Password for the SOCKS proxy. If this is specified, --socks-username
    /// needs to be specified as well.
    #[clap(id = "socks_password", long = "socks-password")]
    pub password: Option<String>,
}

impl SocksArgs {
    pub fn builder(&self) -> Result<socks::Builder, Error> {
        let mut builder = socks::Builder::default().with_bind_address(self.bind_address);

        match (&self.username, &self.password) {
            (Some(username), Some(password)) => {
                builder = builder.with_password(username.clone(), password.clone())
            }
            (None, None) => {}
            _ => bail!("Either both username and password or neither must be specified"),
        }

        Ok(builder)
    }
}

#[derive(Debug, Parser)]
pub struct PcapArgs {
    #[clap(id = "pcap_enabled", long = "pcap")]
    pub enabled: bool,

    #[clap(
        id = "pcap_interface",
        value_name("INTERFACE"),
        long = "pcap-interface"
    )]
    pub interface: Option<String>,

    #[clap(id = "pcap_ap", long = "pcap-ap")]
    pub ap: bool,
}

#[derive(Debug, Parser)]
pub struct ApiArgs {
    #[clap(id = "api_enabled", long = "api")]
    pub enabled: bool,

    #[clap(
        id = "api_bind_address",
        value_name("ADDRESS"),
        long = "api-bind-address",
        default_value = "127.0.0.1:8080",
        env = "SKUNK_API"
    )]
    pub bind_address: SocketAddr,
}

/// Skunk app command-line options (i.e. command-line arguments without the
/// actual command to run).
#[derive(Clone, Debug, Parser)]
pub struct Options {
    /// Path to the skunk configuration directory. Defaults to
    /// `~/.config/gocksec/skunk/`.
    #[clap(short, long, env = "SKUNK_CONFIG")]
    pub config: Option<PathBuf>,
}
