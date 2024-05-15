use std::sync::Arc;

use rustls::{
    ClientConfig,
    RootCertStore,
};

use super::Lazy;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("rustls error")]
    Rustls(#[from] rustls::Error),
}

pub fn default_client_config() -> Result<Arc<ClientConfig>, Error> {
    static CONFIG: Lazy<ClientConfig> = Lazy::new();
    CONFIG.get_or_try_init(|| {
        Ok(ClientConfig::builder()
            .with_root_certificates(native_certificates()?)
            .with_no_client_auth())
    })
}

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
