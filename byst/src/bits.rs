#![allow(dead_code)]

use std::ops::{
    BitAnd,
    BitOr,
    Shl,
    Shr,
};

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
