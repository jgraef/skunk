#[allow(clippy::module_inception)]
pub mod bytes;
pub mod bytes_mut;
//mod spilled;
mod r#static;
pub mod view;

cfg_pub! {
    pub(#[cfg(feature = "bytes-impl")]) mod r#impl;
}

pub use self::{
    bytes::Bytes,
    bytes_mut::BytesMut,
};
use crate::util::cfg_pub;
