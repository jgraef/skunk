use std::path::{
    Path,
    PathBuf,
};

use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair, KeyUsagePurpose
};
use serde::Deserialize;

use super::config::Config;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("rcgen error")]
    Rcgen(#[from] rcgen::Error),
}

#[derive(Debug, Deserialize)]
pub struct CaConfig {
    #[serde(default = "default_config_path")]
    path: PathBuf,
}

fn default_config_path() -> PathBuf {
    "ca".into()
}

impl Default for CaConfig {
    fn default() -> Self {
        Self {
            path: default_config_path(),
        }
    }
}

pub struct Ca {
    key_pair: KeyPair,
    cert: Certificate,
}

impl Ca {
    pub const KEY_FILE: &'static str = "ca-key.pem";
    pub const CERT_FILE: &'static str = "ca-cert.pem";

    pub async fn open(config: &Config) -> Result<Self, Error> {
        let path = config.path.join(&config.config.ca.path);
        let key_pair =
            KeyPair::from_pem(&tokio::fs::read_to_string(path.join("ca-key.pem")).await?)?;
        todo!();
    }

    pub async fn generate(config: &Config) -> Result<Self, Error> {
        let path = config.path.join(&config.config.ca.path);
        if !path.exists() {
            std::fs::create_dir_all(&path)?;
        }

        let mut cert_params = CertificateParams::default();
        cert_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        cert_params.distinguished_name = DistinguishedName::new();
        cert_params.distinguished_name.push(DnType::CommonName, "skunk root ca");
        cert_params.distinguished_name.push(DnType::OrganizationName, "gocksec");
        cert_params.key_usages.push(KeyUsagePurpose::KeyCertSign);

        let (key_pair, cert) = tokio::task::spawn_blocking(move || {
            let key_pair = KeyPair::generate()?;
            let cert = cert_params.self_signed(&key_pair)?;
            Ok::<_, Error>((key_pair, cert))
        })
        .await
        .unwrap()?;

        tokio::fs::write(path.join(Self::KEY_FILE), key_pair.serialize_pem()).await?;
        tokio::fs::write(path.join(Self::CERT_FILE), cert.pem()).await?;

        Ok(Self { key_pair, cert })
    }
}
