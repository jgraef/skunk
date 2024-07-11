use color_eyre::eyre::Error;
use semver::Version;
use semver_macro::env_version;
use skunk::protocol::tls;

use crate::env::{
    args::{
        Command,
        Options,
        ProxyArgs,
    },
    config::TlsConfig,
    Environment,
};

pub const APP_NAME: &'static str = std::env!("CARGO_PKG_NAME");
pub const APP_VERSION: Version = env_version!("CARGO_PKG_VERSION");

pub struct App {
    environment: Environment,
}

impl App {
    pub fn new(options: Options) -> Result<Self, Error> {
        let environment = Environment::from_options(options)?;
        Ok(Self { environment })
    }

    /// Runs the given command-line command.
    pub async fn run(&mut self, command: Command) -> Result<(), Error> {
        match command {
            Command::GenerateCert { force } => {
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
        let tls_config = self
            .environment
            .get_untracked::<TlsConfig>("tls")
            .await?
            .unwrap_or_default();
        let ca = tls::Ca::generate().await?;
        let key_file = self.environment.config_relative_path(&tls_config.key_file);
        let cert_file = self.environment.config_relative_path(&tls_config.cert_file);

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

    async fn proxy(&self, args: ProxyArgs) -> Result<(), Error> {
        crate::proxy::run(self.environment.clone(), args).await
    }
}
