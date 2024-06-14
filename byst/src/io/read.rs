use std::marker::PhantomData;

use byst_macros::for_tuple;
pub use byst_macros::Read;

use super::End;
use crate::buf::BufMut;

/// Something that can be read without specifying endianness.
pub trait Read<R>: Sized {
    fn read(reader: R) -> Result<Self, End>;
}

/// Something that can be read with endianness.
pub trait ReadXe<R, E>: Sized {
    fn read(reader: R) -> Result<Self, End>;
}

/// A reader that can read into a buffer.
///
/// Implementing this will cause [`Read`] and [`ReadXe`] to be implemented for
/// many common types.
pub trait ReadIntoBuf {
    fn read_into_buf<B: BufMut>(&mut self, buf: B) -> Result<(), End>;
}

impl<'r, R: ReadIntoBuf> ReadIntoBuf for &'r mut R {
    #[inline]
    fn read_into_buf<B: BufMut>(&mut self, buf: B) -> Result<(), End> {
        (*self).read_into_buf(buf)
    }
}

impl<R> Read<R> for () {
    #[inline]
    fn read(_reader: R) -> Result<Self, End> {
        Ok(())
    }
}

impl<R, T> Read<R> for PhantomData<T> {
    #[inline]
    fn read(_reader: R) -> Result<Self, End> {
        Ok(PhantomData)
    }
}

impl<R: ReadIntoBuf, const N: usize> Read<R> for [u8; N] {
    #[inline]
    fn read(mut reader: R) -> Result<Self, End> {
        let mut buf = [0u8; N];
        reader.read_into_buf(&mut buf)?;
        Ok(buf)
    }
}

impl<R: ReadIntoBuf> Read<R> for u8 {
    #[inline]
    fn read(mut reader: R) -> Result<Self, End> {
        let mut buf = [0u8; 1];
        reader.read_into_buf(&mut buf)?;
        Ok(buf[0])
    }
}

impl<R: ReadIntoBuf> Read<R> for i8 {
    #[inline]
    fn read(mut reader: R) -> Result<Self, End> {
        let mut buf = [0u8; 1];
        reader.read_into_buf(&mut buf)?;
        Ok(buf[0] as i8)
    }
}

/// Read macro
#[macro_export]
macro_rules! read {
    ($reader:ident => $ty:ty as $endianness:ty) => {
        {
            <$ty as ::byst::io::read::ReadXe::<_, $endianness>>::read(&mut $reader)
        }
    };
    ($reader:ident => $ty:ty) => {
        {
            <$ty as ::byst::io::read::Read::<_>>::read(&mut $reader)
        }
    };
    ($reader:ident as $endianness:ty) => {
        read!($reader => _ as $endianness)
    };
    ($reader:ident) => {
        read!($reader => _)
    };
}
pub use read;

// implement `Read` and `Write` for tuples.
// todo: also implement `ReadXe` and `WriteXe`.
macro_rules! impl_read_for_tuple {
    ($($index:tt => $name:ident: $ty:ident),*) => {
        impl<R, $($ty),*> Read<R> for ($($ty,)*)
        where
            $($ty: for<'r> Read<&'r mut R>,)*
        {
            fn read(mut reader: R) -> Result<Self, End> {
                $(
                    let $name = <$ty as Read<&mut R>>::read(&mut reader)?;
                )*
                Ok(($(
                    $name,
                )*))
            }
        }
    };
}
for_tuple!(impl_read_for_tuple! for 1..=8);
