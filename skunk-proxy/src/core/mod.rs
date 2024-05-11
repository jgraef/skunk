pub mod address;
pub mod connect;
pub mod filter;
pub mod layer;
pub mod tls;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("tls error")]
    Tls(#[from] self::tls::Error),

    #[error("io error")]
    Io(#[from] std::io::Error),
}
