use std::{
    collections::HashMap, fmt::Debug, fs::File, io::BufReader, path::PathBuf, pin::Pin, sync::Arc
};

use futures_util::Future;
use rcgen::{
    BasicConstraints,
    Certificate,
    CertificateParams,
    DistinguishedName,
    DnType,
    IsCa,
    KeyPair,
    KeyUsagePurpose,
    SanType,
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
    TlsConnector,
};

use crate::{
    app::config::Config,
    core::address::HostAddress,
};

use super::layer::Layer;

pub type TlsClientStream<S> = tokio_rustls::client::TlsStream<S>;
pub type TlsServerStream<S> = tokio_rustls::server::TlsStream<S>;

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

    #[error("invalid server name: {host}")]
    InvalidServerName { host: HostAddress },
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
        server_name: String,
    ) -> Result<CertificateDer<'static>, Error> {
        //let mut cert_params = CertificateParams::from_ca_cert_der(&self.cert)?;
        let mut cert_params = CertificateParams::default();
        cert_params.distinguished_name = DistinguishedName::new();
        cert_params
            .distinguished_name
            .push(DnType::CommonName, &server_name);
        cert_params
            .distinguished_name
            .push(DnType::OrganizationName, "gocksec");
        cert_params
            .subject_alt_names
            .push(SanType::DnsName(server_name.try_into()?));

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
pub struct TlsContext {
    client_config: Arc<ClientConfig>,
    certs: Arc<Mutex<HashMap<String, CertificateDer<'static>>>>,
    ca: Ca,
    server_key: Arc<KeyPair>,
}

impl TlsContext {
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
            certs,
            ca,
            server_key,
        })
    }

    pub async fn accept<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        stream: S,
        _host: &HostAddress,
    ) -> Result<TlsServerStream<S>, Error> {
        let start_handshake = LazyConfigAcceptor::new(Acceptor::default(), stream).await?;

        let client_hello = start_handshake.client_hello();
        // todo: we could also use the hostname/ip from the connect call
        // todo: we should use the server name from the original certificate.
        let server_name = client_hello.server_name().ok_or(Error::NoServerName)?;

        let server_cert = {
            let mut certs = self.certs.lock().await;
            if let Some(cert) = certs.get(server_name) {
                cert.clone()
            }
            else {
                let cert = self
                    .ca
                    .sign(self.server_key.clone(), server_name.to_owned())
                    .await?;
                certs.insert(server_name.to_owned(), cert.to_owned());
                cert
            }
        };

        let cert_chain = vec![server_cert, CertificateDer::clone(self.ca.root_cert())];
        let server_key = PrivateKeyDer::try_from(self.server_key.serialize_der()).unwrap();

        let server_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, server_key)
            .unwrap();

        let stream = start_handshake.into_stream(Arc::new(server_config)).await?;

        Ok(stream)
    }

    pub async fn connect<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        stream: S,
        host: &HostAddress,
    ) -> Result<TlsClientStream<S>, Error> {
        let domain = match host {
            HostAddress::IpAddress(ip_address) => ServerName::IpAddress((*ip_address).into()),
            HostAddress::DnsName(name) => {
                ServerName::DnsName(
                    name.to_owned()
                        .try_into()
                        .map_err(|_| Error::InvalidServerName { host: host.clone() })?,
                )
            }
        };

        let stream = TlsConnector::from(self.client_config.clone())
            .connect(domain, stream)
            .await?;

        Ok(stream)
    }
}

#[derive(Clone)]
pub struct TlsLayer<L> {
    inner: L,
    context: TlsContext,
}

impl<L> TlsLayer<L> {
    pub fn new(inner: L, context: TlsContext) -> Self {
        Self {
            inner,
            context,
        }
    }
}

impl<L, S, T> Layer<S, T> for TlsLayer<L>
where
    L: Layer<S, T>,
    S: AsyncRead + AsyncWrite,
    T: AsyncRead + AsyncWrite,
{
    type Future = Pin<Box<dyn Future<Output = Result<(), super::Error>> + Send>>;

    fn layer(&self, source: S, target: T) -> Self::Future {
        Box::pin(async move {
            todo!();
            Ok(())
        })
    }
}