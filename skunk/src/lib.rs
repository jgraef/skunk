#![allow(dead_code)]

pub mod address;
pub mod connect;
pub mod layer;
pub mod proxy;
pub mod rule;
#[cfg(feature = "store")]
pub mod store;
pub mod util;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error")]
    Io(#[from] std::io::Error),

    #[cfg(feature = "layer-tls")]
    #[error("tls error")]
    Tls(#[from] self::layer::tls::Error),

    #[cfg(feature = "layer-http")]
    #[error("http error")]
    Http(#[from] self::layer::http::Error),

    #[cfg(feature = "store")]
    #[error("store error")]
    Store(#[from] self::store::Error),
}
