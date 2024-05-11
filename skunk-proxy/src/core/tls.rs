use std::{
    collections::HashMap,
    fmt::Debug,
    fs::File,
    future::Future,
    io::BufReader,
    net::IpAddr,
    path::PathBuf,
    str::FromStr,
    sync::Arc,
};

use rcgen::{
    BasicConstraints,
    Certificate,
    CertificateParams,
    DistinguishedName,
    DnType,
    IsCa,
    KeyPair,
    KeyUsagePurpose,
};
use rustls::{
    pki_types::{
        CertificateDer,
        PrivateKeyDer,
        ServerName,
    },
    server::Acceptor,
    ClientConfig,
    RootCertStore,
    ServerConfig,
};
use serde::Deserialize;
use tokio::{
    io::{
        AsyncRead,
        AsyncWrite,
    },
    sync::Mutex,
};
use tokio_rustls::{
    LazyConfigAcceptor,
    StartHandshake,
    TlsConnector,
};

use crate::{
    app::config::Config,
    core::layer::Layer,
};

pub type ClientStream<S> = tokio_rustls::client::TlsStream<S>;
pub type ServerStream<S> = tokio_rustls::server::TlsStream<S>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("rcgen error")]
    Rcgen(#[from] rcgen::Error),

    #[error("missing certificate: {path}")]
    NoCertificate { path: PathBuf },

    #[error("client didn't send a server name")]
    NoServerName,

    #[error("invalid server name: {hostname}")]
    InvalidServerName { hostname: String },

    #[error("the target server didn't send a server certificate chain")]
    NoTargetCertificate,
}

impl From<Error> for std::io::Error {
    fn from(e: Error) -> Self {
        match e {
            Error::Io(e) => e,
            _ => std::io::Error::new(std::io::ErrorKind::Other, e),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CaConfig {
    #[serde(default = "default_config_key_file")]
    key_file: PathBuf,

    #[serde(default = "default_config_cert_file")]
    cert_file: PathBuf,
}

fn default_config_key_file() -> PathBuf {
    "ca.key.pem".into()
}

fn default_config_cert_file() -> PathBuf {
    "ca.cert.pem".into()
}

impl Default for CaConfig {
    fn default() -> Self {
        Self {
            key_file: default_config_key_file(),
            cert_file: default_config_cert_file(),
        }
    }
}

#[derive(Clone)]
pub struct Ca {
    key_pair: Arc<KeyPair>,
    cert: Arc<CertificateDer<'static>>,
    cert_for_signing: Arc<Certificate>,
}

impl Ca {
    pub fn open(config: &Config) -> Result<Self, Error> {
        let key_path = config.path.join(&config.config.ca.key_file);
        let key_pair = Arc::new(KeyPair::from_pem(&std::fs::read_to_string(&key_path)?)?);

        let cert_path = config.path.join(&config.config.ca.cert_file);
        let mut reader = BufReader::new(File::open(&cert_path)?);
        let cert = Arc::new(
            rustls_pemfile::certs(&mut reader)
                .next()
                .ok_or_else(move || Error::NoCertificate { path: cert_path })??,
        );

        // see https://github.com/rustls/rcgen/issues/268
        let cert_params = CertificateParams::from_ca_cert_der(&cert)?;
        let cert_for_signing = Arc::new(cert_params.self_signed(&key_pair)?);

        Ok(Self {
            key_pair,
            cert,
            cert_for_signing,
        })
    }

    pub async fn generate(config: &Config) -> Result<Self, Error> {
        let mut cert_params = CertificateParams::default();
        cert_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        cert_params.distinguished_name = DistinguishedName::new();
        cert_params
            .distinguished_name
            .push(DnType::CommonName, "skunk root ca");
        cert_params
            .distinguished_name
            .push(DnType::OrganizationName, "gocksec");
        cert_params.key_usages.push(KeyUsagePurpose::KeyCertSign);
        cert_params
            .key_usages
            .push(KeyUsagePurpose::DigitalSignature);

        let (key_pair, cert_for_signing) = tokio::task::spawn_blocking(move || {
            let key_pair = Arc::new(KeyPair::generate()?);
            let cert_for_signing = Arc::new(cert_params.self_signed(&key_pair)?);
            Ok::<_, Error>((key_pair, cert_for_signing))
        })
        .await
        .unwrap()?;

        tokio::fs::write(
            config.path.join(&config.config.ca.key_file),
            key_pair.serialize_pem(),
        )
        .await?;
        tokio::fs::write(
            config.path.join(&config.config.ca.cert_file),
            cert_for_signing.pem(),
        )
        .await?;

        Ok(Self {
            key_pair,
            cert: Arc::new(cert_for_signing.der().to_owned()),
            cert_for_signing,
        })
    }

    pub async fn sign(
        &self,
        server_key: Arc<KeyPair>,
        cert_params: CertificateParams,
    ) -> Result<CertificateDer<'static>, Error> {
        /*let mut cert_params = CertificateParams::default();
        cert_params.distinguished_name = DistinguishedName::new();
        cert_params
            .distinguished_name
            .push(DnType::CommonName, &server_name);
        cert_params
            .distinguished_name
            .push(DnType::OrganizationName, "gocksec");
        cert_params
            .subject_alt_names
            .push(SanType::DnsName(server_name.try_into()?));*/

        let ca_key = self.key_pair.clone();
        let ca_cert = self.cert_for_signing.clone();

        let server_cert = tokio::task::spawn_blocking(move || {
            cert_params.signed_by(&server_key, &ca_cert, &ca_key)
        })
        .await
        .unwrap()?;

        Ok(server_cert.into())
    }

    pub fn root_cert(&self) -> &Arc<CertificateDer<'static>> {
        &self.cert
    }
}

impl Debug for Ca {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ca")
            .field("key_pair", &self.key_pair)
            .field("cert", &self.cert)
            .finish()
    }
}

#[derive(Clone, Debug)]
struct ServerContext {
    certs: Arc<Mutex<HashMap<String, CertificateDer<'static>>>>,
    ca: Ca,
    server_key: Arc<KeyPair>,
}

#[derive(Clone, Debug)]
pub struct Context {
    client_config: Arc<ClientConfig>,
    server_context: ServerContext,
}

impl Context {
    pub async fn new(ca: Ca) -> Result<Self, Error> {
        let client_config = Arc::new(
            ClientConfig::builder()
                .with_root_certificates(RootCertStore::empty()) // todo
                .with_no_client_auth(),
        );

        let certs = Arc::new(Mutex::new(HashMap::new()));

        let server_key =
            tokio::task::spawn_blocking(|| Ok::<_, Error>(Arc::new(KeyPair::generate()?)))
                .await
                .unwrap()?;

        Ok(Self {
            client_config,
            server_context: ServerContext {
                certs,
                ca,
                server_key,
            },
        })
    }

    pub async fn start_accept<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        stream: S,
    ) -> Result<Accept<S>, Error> {
        let start_handshake = LazyConfigAcceptor::new(Acceptor::default(), stream).await?;
        Ok(Accept {
            start_handshake,
            server_context: self.server_context.clone(),
        })
    }

    pub async fn connect<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        stream: S,
        domain: ServerName<'static>,
    ) -> Result<ClientStream<S>, Error> {
        let stream = TlsConnector::from(self.client_config.clone())
            .connect(domain, stream)
            .await?;

        Ok(stream)
    }
}

pub struct Accept<S> {
    start_handshake: StartHandshake<S>,
    server_context: ServerContext,
}

impl<S> Accept<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub async fn finish(
        self,
        server_name: &str,
        cert_params: CertificateParams,
    ) -> Result<ServerStream<S>, Error> {
        let server_cert = {
            let mut certs = self.server_context.certs.lock().await;
            if let Some(cert) = certs.get(server_name) {
                cert.clone()
            }
            else {
                let cert = self
                    .server_context
                    .ca
                    .sign(self.server_context.server_key.clone(), cert_params)
                    .await?;
                certs.insert(server_name.to_owned(), cert.to_owned());
                cert
            }
        };

        let cert_chain = vec![
            server_cert,
            CertificateDer::clone(self.server_context.ca.root_cert()),
        ];
        let server_key =
            PrivateKeyDer::try_from(self.server_context.server_key.serialize_der()).unwrap();

        let server_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, server_key)
            .unwrap();

        let stream = self
            .start_handshake
            .into_stream(Arc::new(server_config))
            .await?;

        Ok(stream)
    }

    pub fn server_name(&self) -> Option<String> {
        let client_hello = self.start_handshake.client_hello();
        client_hello.server_name().map(ToOwned::to_owned)
    }
}

#[derive(Clone)]
pub struct Tls<L> {
    inner: L,
    context: Context,
}

impl<L> Tls<L> {
    pub fn new(inner: L, context: Context) -> Self {
        Self { inner, context }
    }
}

impl<L, S, T> Layer<S, T> for Tls<L>
where
    L: Layer<ServerStream<S>, ClientStream<T>> + Sync,
    S: AsyncRead + AsyncWrite + Send + Unpin,
    T: AsyncRead + AsyncWrite + Send + Unpin,
{
    type Output = ();

    async fn layer(&self, source: S, target: T) -> Result<(), crate::core::Error> {
        async fn handshake<S, T>(
            context: &Context,
            source: S,
            target: T,
        ) -> Result<(ServerStream<S>, ClientStream<T>), Error>
        where
            S: AsyncRead + AsyncWrite + Send + Unpin,
            T: AsyncRead + AsyncWrite + Send + Unpin,
        {
            // start the tls handshake with the source
            let source_accept = context.start_accept(source).await?;

            // get the server_name provided by the TLS client at the source
            // todo: what do we do, if the client didn't provide a server name? we need that
            // to connect to the target. we could also use the `TcpAddress` we
            // get from the proxy layer.
            let source_server_name = source_accept.server_name().ok_or(Error::NoServerName)?;
            let domain = match IpAddr::from_str(&source_server_name) {
                Ok(ip_address) => ServerName::IpAddress(ip_address.into()),
                Err(_) => {
                    ServerName::DnsName(source_server_name.to_owned().try_into().map_err(|_| {
                        Error::InvalidServerName {
                            hostname: source_server_name.clone(),
                        }
                    })?)
                }
            };

            // connect to the target
            let target = context.connect(target, domain).await?;

            // extract certificate parameters from the server certificate we got from the
            // target.
            let target_cert = target
                .get_ref()
                .1
                .peer_certificates()
                .and_then(|certs| certs.first())
                .ok_or(Error::NoTargetCertificate)?;
            // although the name suggest that this method parses *ca* certs, it seems to
            // just extract some of the certificate information.
            let target_cert_params = CertificateParams::from_ca_cert_der(target_cert)?;

            // finish the TLS handshake with the source by imitating the certificate and
            // signing it with out CA.
            let source = source_accept
                .finish(&source_server_name, target_cert_params)
                .await?;

            Ok((source, target))
        }

        let (source, target) = handshake(&self.context, source, target).await?;
        self.inner.layer(source, target).await?;
        Ok(())
    }
}
