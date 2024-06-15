//! bytes, bytter, `byst`!
//!
//! Read and write bytes on steriods!

// required by `crate::buf::partially_initialized`.
#![feature(maybe_uninit_slice, maybe_uninit_write_slice, maybe_uninit_fill)]
// required by `crate::util::bytes::endianness::{Encode, Decode}`.
#![allow(incomplete_features)]
#![feature(generic_const_exprs)]
// required by `crate::buf::slab`
#![feature(new_uninit, slice_ptr_get)]

pub mod buf;
mod bytes;
mod dyn_impl;
pub mod endianness;
pub mod hexdump;
pub mod io;
mod range;
pub mod slab;
pub mod util;

use std::ops::{
    BitAnd,
    BitOr,
    Shl,
    Shr,
};

pub use self::{
    buf::{
        Buf,
        BufMut,
    },
    bytes::{
        Bytes,
        Sbytes,
    },
    range::{
        Range,
        RangeOutOfBounds,
    },
};

// hack to get the proc-macro working from this crate
extern crate self as byst;

pub trait BitFieldExtract<O> {
    fn extract(&self, start: usize, bits: usize) -> O;
}

// todo: I'd really like to do this at compile-time :/
pub fn bit_mask<T>(mut bits: usize) -> T
where
    T: Shl<usize, Output = T> + BitOr<T, Output = T> + From<u8>,
{
    let mut bit_mask = T::from(0u8);
    while bits > 0 {
        bit_mask = (bit_mask << 1) | T::from(1u8);
        bits -= 1;
    }
    bit_mask
}

pub fn extract_bits<T>(value: T, start: usize, bits: usize) -> T
where
    T: Shl<usize, Output = T>
        + BitOr<T, Output = T>
        + From<u8>
        + Shr<usize, Output = T>
        + BitAnd<T, Output = T>,
{
    (value >> start) & bit_mask::<T>(bits)
}

macro_rules! impl_bit_field_extract {
    {
        $(
            $from:ty => {$($to:ty),*};
        )*
    } => {
        $(
            $(
                impl BitFieldExtract<$to> for $from {
                    fn extract(&self, start: usize, bits: usize) -> $to {
                        // ideally this check would also happen at compile-time. after all we know how many bits the int will have.
                        assert!(bits < <$to>::BITS as usize);
                        <$to>::try_from(extract_bits::<Self>(*self, start, bits)).unwrap_or_else(|_| panic!("Can't convert from {} ({}) to {}", stringify!($ty), *self, stringify!($to)))
                    }
                }
            )*

            impl BitFieldExtract<bool> for $from {
                fn extract(&self, start: usize, bits: usize) -> bool {
                    // ideally this check would also happen at compile-time. after all we know how many bits the int will have.
                    assert_eq!(bits, 1);
                    extract_bits::<Self>(*self, start, bits) != 0
                }
            }
        )*
    };
}

impl_bit_field_extract! {
    u8 => {u8};
    u16 => {u8, u16};
    u32 => {u8, u16, u32};
    u64 => {u8, u16, u32, u64};
    u128 => {u8, u16, u32, u64, u128};
}
