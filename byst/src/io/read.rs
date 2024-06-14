use std::{
    marker::PhantomData,
    net::{
        Ipv4Addr,
        Ipv6Addr,
    },
};

use byst_macros::for_tuple;
pub use byst_macros::Read;

use super::End;
use crate::buf::BufMut;

/// Something that can be read from a reader `R`, given the parameters `P`.
pub trait Read<R, P>: Sized {
    fn read(reader: R, parameters: P) -> Result<Self, End>;
}

/// A reader that can read into a buffer.
///
/// Implementing this will cause [`Read`] to be implemented for
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

impl<R> Read<R, ()> for () {
    #[inline]
    fn read(_reader: R, _parameters: ()) -> Result<Self, End> {
        Ok(())
    }
}

impl<R, T> Read<R, ()> for PhantomData<T> {
    #[inline]
    fn read(_reader: R, _parameters: ()) -> Result<Self, End> {
        Ok(PhantomData)
    }
}

impl<R: ReadIntoBuf, const N: usize> Read<R, ()> for [u8; N] {
    #[inline]
    fn read(mut reader: R, _parameters: ()) -> Result<Self, End> {
        let mut buf = [0u8; N];
        reader.read_into_buf(&mut buf)?;
        Ok(buf)
    }
}

impl<R: ReadIntoBuf> Read<R, ()> for u8 {
    #[inline]
    fn read(mut reader: R, _parameters: ()) -> Result<Self, End> {
        let mut buf = [0u8; 1];
        reader.read_into_buf(&mut buf)?;
        Ok(buf[0])
    }
}

impl<R: ReadIntoBuf> Read<R, ()> for i8 {
    #[inline]
    fn read(mut reader: R, _parameters: ()) -> Result<Self, End> {
        let mut buf = [0u8; 1];
        reader.read_into_buf(&mut buf)?;
        Ok(buf[0] as i8)
    }
}

impl<R: ReadIntoBuf> Read<R, ()> for Ipv4Addr {
    #[inline]
    fn read(mut reader: R, _parameters: ()) -> Result<Self, End> {
        let mut buf = [0u8; 4];
        reader.read_into_buf(&mut buf)?;
        Ok(Ipv4Addr::from(buf))
    }
}

impl<R: ReadIntoBuf> Read<R, ()> for Ipv6Addr {
    #[inline]
    fn read(mut reader: R, _parameters: ()) -> Result<Self, End> {
        let mut buf = [0u8; 16];
        reader.read_into_buf(&mut buf)?;
        Ok(Ipv6Addr::from(buf))
    }
}

// implement `Read` and `Write` for tuples.
macro_rules! impl_read_for_tuple {
    ($($index:tt => $name:ident: $ty:ident),*) => {
        impl<R, $($ty),*> Read<R, ()> for ($($ty,)*)
        where
            $($ty: for<'r> Read<&'r mut R, ()>,)*
        {
            fn read(mut reader: R, _parameters: ()) -> Result<Self, End> {
                $(
                    let $name = <$ty as Read<&mut R, ()>>::read(&mut reader, ())?;
                )*
                Ok(($(
                    $name,
                )*))
            }
        }
    };
}
for_tuple!(impl_read_for_tuple! for 1..=8);

/// Read macro
#[macro_export]
macro_rules! read {
    ($reader:ident => $ty:ty; $params:expr) => {
        {
            <$ty as ::byst::io::read::Read::<_, _>>::read(&mut $reader, $params)
        }
    };
    ($reader:ident => $ty:ty) => {
        read!($reader => $ty; ())
    };
    ($reader:ident; $params:expr) => {
        read!($reader => _; $params)
    };
    ($reader:ident) => {
        read!($reader => _; ())
    };
}
pub use read;
