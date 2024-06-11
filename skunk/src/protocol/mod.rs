//! Protocol implementations.

#[cfg(feature = "http")]
pub mod http;
#[cfg(feature = "tls")]
pub mod tls;

// todo: feature flag
pub mod ethernet;
