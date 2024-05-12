pub mod address;
pub mod connect;
pub mod filter;
pub mod layer;
pub mod protocol;
pub mod proxy;
#[cfg(feature = "store")]
pub mod store;
#[cfg(feature = "tls")]
pub mod tls;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error")]
    Io(#[from] std::io::Error),

    #[cfg(feature = "tls")]
    #[error("tls error")]
    Tls(#[from] self::tls::Error),

    #[cfg(feature = "http")]
    #[error("http error")]
    Http(#[from] self::protocol::http::Error),

    #[cfg(feature = "store")]
    #[error("store error")]
    Store(#[from] self::store::Error),
}
