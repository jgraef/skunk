//! TLS connections.
//!
//! This is mainly used to decrypt TLS (e.g. HTTPS) traffic. Note, that in order
//! for a client to accept the modified certificates, the skunk root certificate
//! needs to be installed.

use std::{
    collections::HashMap,
    fmt::Debug,
    fs::File,
    io::BufReader,
    net::IpAddr,
    path::{
        Path,
        PathBuf,
    },
    pin::Pin,
    str::FromStr,
    sync::Arc,
    task::Poll,
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
use tokio::{
    io::{
        AsyncRead,
        AsyncWrite,
        ReadBuf,
    },
    sync::Mutex,
};
use tokio_rustls::{
    LazyConfigAcceptor,
    StartHandshake,
    TlsConnector,
};

use crate::util::Lazy;

/// TLS error type
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("rcgen error")]
    Rcgen(#[from] rcgen::Error),

    #[error("rustls error")]
    Rustls(#[from] rustls::Error),

    #[error("missing certificate: {path}")]
    NoCertificate { path: PathBuf },

    #[error("client didn't send a server name")]
    NoServerName,

    #[error("invalid server name: {hostname}")]
    InvalidServerName { hostname: String },

    #[error("the target server didn't send a server certificate chain")]
    NoTargetCertificate,
}

/// A certificate authority
#[derive(Clone)]
pub struct Ca {
    key_pair: Arc<KeyPair>,
    cert: Arc<CertificateDer<'static>>,
    cert_for_signing: Arc<Certificate>,
}

impl Ca {
    /// Create a CA by reading key and certificate from a file.
    pub fn open(key_file: impl AsRef<Path>, cert_file: impl AsRef<Path>) -> Result<Self, Error> {
        let key_pair = Arc::new(KeyPair::from_pem(&std::fs::read_to_string(key_file)?)?);

        let cert_file = cert_file.as_ref();
        let mut reader = BufReader::new(File::open(cert_file)?);
        let cert = Arc::new(rustls_pemfile::certs(&mut reader).next().ok_or_else(
            move || {
                Error::NoCertificate {
                    path: cert_file.to_owned(),
                }
            },
        )??);

        // we need to create a `Certificate` from the `CertificateDer`. This is not
        // possible. But only certain parameters from the `Certificate` are used
        // for signing, so it doesn't matter that we just sign a new one with the right
        // parameters. see https://github.com/rustls/rcgen/issues/268
        let cert_params = CertificateParams::from_ca_cert_der(&cert)?;
        let cert_for_signing = Arc::new(cert_params.self_signed(&key_pair)?);

        Ok(Self {
            key_pair,
            cert,
            cert_for_signing,
        })
    }

    /// Generate a new CA with a random key.
    pub async fn generate() -> Result<Self, Error> {
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

        Ok(Self {
            key_pair,
            cert: Arc::new(cert_for_signing.der().to_owned()),
            cert_for_signing,
        })
    }

    /// Save this CA's key and certificate to files.
    ///
    /// # FIXME
    ///
    /// This saves the `cert_for_signing`, which isn't actually the correct
    /// cert, if we didn't generate this CA.
    pub fn save(
        &self,
        key_file: impl AsRef<Path>,
        cert_file: impl AsRef<Path>,
    ) -> Result<(), Error> {
        std::fs::write(key_file, self.key_pair.serialize_pem())?;
        std::fs::write(cert_file, self.cert_for_signing.pem())?;
        Ok(())
    }

    /// Create a certificate signed by this CA.
    pub async fn sign(
        &self,
        server_key: Arc<KeyPair>,
        mut cert_params: CertificateParams,
    ) -> Result<CertificateDer<'static>, Error> {
        // since we generate new certificates during each session, but with the same
        // issuer, we need to generate a random serial number.
        cert_params.serial_number = None;

        let ca_key = self.key_pair.clone();
        let ca_cert = self.cert_for_signing.clone();

        let server_cert = tokio::task::spawn_blocking(move || {
            cert_params.signed_by(&server_key, &ca_cert, &ca_key)
        })
        .await
        .unwrap()?;

        Ok(server_cert.into())
    }

    /// Return the CA's root certificate.
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

/// General TLS context that can be used to create server and client
/// connections.
///
/// You're probably interested in the [`Context::decrypt`] method.
#[derive(Clone, Debug)]
pub struct Context {
    pub(crate) client_config: Arc<ClientConfig>,
    server_context: ServerContext,
}

impl Context {
    /// Create context from [`Ca`].
    pub async fn new(ca: Ca) -> Result<Self, Error> {
        let certs = Arc::new(Mutex::new(HashMap::new()));

        let server_key =
            tokio::task::spawn_blocking(|| Ok::<_, Error>(Arc::new(KeyPair::generate()?)))
                .await
                .unwrap()?;

        Ok(Self {
            client_config: default_client_config()?,
            server_context: ServerContext {
                certs,
                ca,
                server_key,
            },
        })
    }

    /// Start accepting a TLS server connection.
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

    /// Create a TLS client connection.
    pub async fn connect<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        stream: S,
        domain: ServerName<'static>,
    ) -> Result<Outgoing<S>, Error> {
        let stream = TlsConnector::from(self.client_config.clone())
            .connect(domain, stream)
            .await?;

        Ok(Outgoing { inner: stream })
    }

    /// Decrypt the incoming connection by presenting our own certificate.
    ///
    /// This first establishes the outgoing connection to get the certificate
    /// from the actual server. This certificate is then modified and signed by
    /// our CA. The modified certificate is presented to the client.
    pub async fn decrypt<I, O>(
        &self,
        incoming: I,
        outgoing: O,
    ) -> Result<(Incoming<I>, Outgoing<O>), Error>
    where
        I: AsyncRead + AsyncWrite + Unpin,
        O: AsyncRead + AsyncWrite + Unpin,
    {
        // start the tls handshake with the source
        let source_accept = self.start_accept(incoming).await?;

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
        let target = self.connect(outgoing, domain).await?;

        // extract certificate parameters from the server certificate we got from the
        // target.
        let target_cert = target
            .inner
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|certs| certs.first())
            .ok_or(Error::NoTargetCertificate)?;
        // although the name suggest that this method parses *ca* certs, it seems to
        // just extract some of the certificate information.
        let target_cert_params = CertificateParams::from_ca_cert_der(target_cert)?;

        // finish the TLS handshake with the source by imitating the certificate and
        // signing it with our CA.
        let source = source_accept
            .finish(&source_server_name, target_cert_params)
            .await?;

        Ok((source, target))
    }

    /// Maybe decrypts TLS traffic. This is a convenience function that returns
    /// a single type regardless of whether encryption is used or not.
    pub async fn maybe_decrypt<I, O>(
        &self,
        incoming: I,
        outgoing: O,
        decrypt: bool,
    ) -> Result<(maybe::Incoming<I>, maybe::Outgoing<O>), Error>
    where
        I: AsyncRead + AsyncWrite + Unpin,
        O: AsyncRead + AsyncWrite + Unpin,
    {
        let pair = if decrypt {
            let (incoming, outgoing) = self.decrypt(incoming, outgoing).await?;
            (
                maybe::Incoming::Encrypted(incoming),
                maybe::Outgoing::Encrypted(outgoing),
            )
        }
        else {
            (
                maybe::Incoming::Unencrypted(incoming),
                maybe::Outgoing::Unencrypted(outgoing),
            )
        };
        Ok(pair)
    }
}

impl From<Context> for Arc<ClientConfig> {
    fn from(value: Context) -> Self {
        value.client_config.clone()
    }
}

/// Process of accepting a TLS server connection
pub struct Accept<S> {
    start_handshake: StartHandshake<S>,
    server_context: ServerContext,
}

impl<S> Accept<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    /// Finish the TLS handshake.
    ///
    /// The `cert_params` argument will be used to create a certificate signed
    /// by the skunk CA that is presented to the client. The `server_name`
    /// is used to cache certificates.
    pub async fn finish(
        self,
        server_name: &str,
        cert_params: CertificateParams,
    ) -> Result<Incoming<S>, Error> {
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
            //CertificateDer::clone(self.server_context.ca.root_cert()),
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

        Ok(Incoming { inner: stream })
    }

    /// The server name that was sent by the client in the `CLIENT_HELLO`
    /// message.
    pub fn server_name(&self) -> Option<String> {
        let client_hello = self.start_handshake.client_hello();
        client_hello.server_name().map(ToOwned::to_owned)
    }
}

/// An outgoing (client) connection that is TLS encrypted.
#[derive(Debug)]
pub struct Outgoing<Inner> {
    inner: tokio_rustls::client::TlsStream<Inner>,
}

impl<Inner> Outgoing<Inner> {
    pub fn get_tls_connection(&self) -> &rustls::ClientConnection {
        &self.inner.get_ref().1
    }
}

impl<Inner: AsyncRead + AsyncWrite + Unpin> AsyncRead for Outgoing<Inner> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<Inner: AsyncRead + AsyncWrite + Unpin> AsyncWrite for Outgoing<Inner> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// An incoming (server) connection that is TLS encrypted.
#[derive(Debug)]
pub struct Incoming<Inner> {
    inner: tokio_rustls::server::TlsStream<Inner>,
}

impl<Inner> Incoming<Inner> {
    pub fn get_tls_connection(&self) -> &rustls::ServerConnection {
        &self.inner.get_ref().1
    }
}

impl<Inner: AsyncRead + AsyncWrite + Unpin> AsyncRead for Incoming<Inner> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<Inner: AsyncRead + AsyncWrite + Unpin> AsyncWrite for Incoming<Inner> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

pub mod maybe {
    //! Stream types that represent connections that are either encrypted or
    //! unencrypted.

    use std::{
        ops::DerefMut,
        pin::Pin,
        task::Poll,
    };

    use tokio::io::{
        AsyncRead,
        AsyncWrite,
        ReadBuf,
    };

    /// An outgoing (client) connection that might be TLS encrypted.
    #[derive(Debug)]
    pub enum Outgoing<Inner> {
        Encrypted(super::Outgoing<Inner>),
        Unencrypted(Inner),
    }

    impl<Inner> Outgoing<Inner> {
        pub fn get_tls_connection(&self) -> Option<&rustls::ClientConnection> {
            match self {
                Outgoing::Encrypted(inner) => Some(inner.get_tls_connection()),
                Outgoing::Unencrypted(_) => None,
            }
        }
    }

    impl<Inner: AsyncRead + AsyncWrite + Unpin> AsyncRead for Outgoing<Inner> {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            match self.deref_mut() {
                Outgoing::Encrypted(inner) => Pin::new(inner).poll_read(cx, buf),
                Outgoing::Unencrypted(inner) => Pin::new(inner).poll_read(cx, buf),
            }
        }
    }

    impl<Inner: AsyncRead + AsyncWrite + Unpin> AsyncWrite for Outgoing<Inner> {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, std::io::Error>> {
            match self.deref_mut() {
                Outgoing::Encrypted(inner) => Pin::new(inner).poll_write(cx, buf),
                Outgoing::Unencrypted(inner) => Pin::new(inner).poll_write(cx, buf),
            }
        }

        fn poll_flush(
            mut self: Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> Poll<Result<(), std::io::Error>> {
            match self.deref_mut() {
                Outgoing::Encrypted(inner) => Pin::new(inner).poll_flush(cx),
                Outgoing::Unencrypted(inner) => Pin::new(inner).poll_flush(cx),
            }
        }

        fn poll_shutdown(
            mut self: Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> Poll<Result<(), std::io::Error>> {
            match self.deref_mut() {
                Outgoing::Encrypted(inner) => Pin::new(inner).poll_shutdown(cx),
                Outgoing::Unencrypted(inner) => Pin::new(inner).poll_shutdown(cx),
            }
        }
    }

    /// An incoming (server) connection that might be TLS encrypted.
    #[derive(Debug)]
    pub enum Incoming<Inner> {
        Encrypted(super::Incoming<Inner>),
        Unencrypted(Inner),
    }

    impl<Inner> Incoming<Inner> {
        pub fn get_tls_connection(&self) -> Option<&rustls::ServerConnection> {
            match self {
                Incoming::Encrypted(inner) => Some(inner.get_tls_connection()),
                Incoming::Unencrypted(_) => None,
            }
        }
    }

    impl<Inner: AsyncRead + AsyncWrite + Unpin> AsyncRead for Incoming<Inner> {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            match self.deref_mut() {
                Incoming::Encrypted(inner) => Pin::new(inner).poll_read(cx, buf),
                Incoming::Unencrypted(inner) => Pin::new(inner).poll_read(cx, buf),
            }
        }
    }

    impl<Inner: AsyncRead + AsyncWrite + Unpin> AsyncWrite for Incoming<Inner> {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, std::io::Error>> {
            match self.deref_mut() {
                Incoming::Encrypted(inner) => Pin::new(inner).poll_write(cx, buf),
                Incoming::Unencrypted(inner) => Pin::new(inner).poll_write(cx, buf),
            }
        }

        fn poll_flush(
            mut self: Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> Poll<Result<(), std::io::Error>> {
            match self.deref_mut() {
                Incoming::Encrypted(inner) => Pin::new(inner).poll_flush(cx),
                Incoming::Unencrypted(inner) => Pin::new(inner).poll_flush(cx),
            }
        }

        fn poll_shutdown(
            mut self: Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> Poll<Result<(), std::io::Error>> {
            match self.deref_mut() {
                Incoming::Encrypted(inner) => Pin::new(inner).poll_shutdown(cx),
                Incoming::Unencrypted(inner) => Pin::new(inner).poll_shutdown(cx),
            }
        }
    }
}

/// Returns the default TLS client config. This uses the natively installed root
/// certificates from [`native_certificates`].
pub fn default_client_config() -> Result<Arc<ClientConfig>, Error> {
    static CONFIG: Lazy<ClientConfig> = Lazy::new();
    CONFIG.get_or_try_init(|| {
        Ok(ClientConfig::builder()
            .with_root_certificates(native_certificates()?)
            .with_no_client_auth())
    })
}

/// Loads root certificates from the system that can be used as trust anchors.
/// This only loads the certificates on the first call and will cache the
/// result.
pub fn native_certificates() -> Result<Arc<RootCertStore>, Error> {
    static CERTS: Lazy<RootCertStore> = Lazy::new();
    CERTS.get_or_try_init(|| {
        let mut certs = RootCertStore::empty();
        for cert in rustls_native_certs::load_native_certs()? {
            certs.add(cert)?;
        }
        Ok(certs)
    })
}
