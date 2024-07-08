use semver::Version;

#[cfg(feature = "axum")]
mod axum;
pub mod error;
pub mod flows;
pub mod socket;
pub mod util;

pub const PROTOCOL_VERSION: Version = Version::new(0, 1, 0);
