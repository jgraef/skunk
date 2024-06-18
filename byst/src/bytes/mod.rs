pub mod bytes;
pub mod bytes_mut;
mod spilled;

#[cfg(not(feature = "bytes-impl"))]
pub(crate) mod r#impl;
#[cfg(feature = "bytes-impl")]
pub mod r#impl;

pub use self::{
    bytes::Bytes,
    bytes_mut::BytesMut,
};
