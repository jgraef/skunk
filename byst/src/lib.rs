//! bytes, bytter, `byst`!
//!
//! Read and write bytes on steriods!

// enabled nightly features
// todo: we might get around these by just copying their source code.

// required by `crate::buf::partially_initialized`.
#![feature(maybe_uninit_slice, maybe_uninit_write_slice, maybe_uninit_fill)]
// required by `crate::buf::array_vec`
#![feature(maybe_uninit_array_assume_init)]
// required by `crate::endianness::{Encode, Decode}`.
#![allow(incomplete_features)]
#![feature(generic_const_exprs)]
// required by `crate::buf::slab`
#![feature(new_uninit, slice_ptr_get)]

mod bits;
pub mod buf;
pub mod bytes;
mod copy;
pub mod endianness;
pub mod hexdump;
pub mod io;
mod range;
pub mod util;

pub use self::{
    buf::{
        Buf,
        BufMut,
    },
    bytes::{
        Bytes,
        BytesMut,
    },
    copy::{
        copy,
        copy_io,
        copy_range,
    },
    range::{
        Range,
        RangeOutOfBounds,
    },
};

// hack to get the proc-macro working from this crate
extern crate self as byst;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("Index out of bounds: {required} not in buffer ({}..{})", .bounds.0, .bounds.1)]
pub struct IndexOutOfBounds {
    pub required: usize,
    pub bounds: (usize, usize),
}
