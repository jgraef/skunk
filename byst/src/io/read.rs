use std::{
    convert::Infallible,
    marker::PhantomData,
    net::{
        Ipv4Addr,
        Ipv6Addr,
    },
};

use byst_macros::for_tuple;
pub use byst_macros::Read;

use crate::{
    buf::{
        copy::CopyError,
        BufMut,
    },
    RangeOutOfBounds,
};

/// TODO: move this somewhere else, since `Read` now has an assoociated type for
/// the error.
#[derive(Clone, Copy, Debug, Default, thiserror::Error)]
#[error("End of reader")]
pub struct End;

impl End {
    pub fn from_copy_error(e: CopyError) -> Self {
        match e {
            CopyError::SourceRangeOutOfBounds(_) => Self,
            _ => {
                panic!("Unexpected error while copying: {e}");
            }
        }
    }

    pub fn from_range_out_of_bounds(_: RangeOutOfBounds) -> Self {
        // todo: we could do some checks here, if it's really an error that can be
        // interpreted as end of buffer.
        Self
    }
}

impl From<End> for std::io::ErrorKind {
    fn from(_: End) -> Self {
        std::io::ErrorKind::UnexpectedEof
    }
}

impl From<End> for std::io::Error {
    fn from(_: End) -> Self {
        std::io::ErrorKind::UnexpectedEof.into()
    }
}

impl From<Infallible> for End {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}

/// Something that can be read from a reader `R`, given the parameters `P`.
pub trait Read<R, P>: Sized {
    type Error;

    fn read(reader: &mut R, parameters: P) -> Result<Self, Self::Error>;
}

/// A reader that can read into a buffer.
///
/// Implementing this will cause [`Read`] to be implemented for
/// many common types.
pub trait ReadIntoBuf {
    type Error;

    fn read_into_buf<B: BufMut>(&mut self, buf: B) -> Result<(), Self::Error>;
}

impl<'r, R: ReadIntoBuf> ReadIntoBuf for &'r mut R {
    type Error = <R as ReadIntoBuf>::Error;

    #[inline]
    fn read_into_buf<B: BufMut>(&mut self, buf: B) -> Result<(), Self::Error> {
        (*self).read_into_buf(buf)
    }
}

impl<R> Read<R, ()> for () {
    type Error = Infallible;

    #[inline]
    fn read(_reader: &mut R, _parameters: ()) -> Result<Self, Self::Error> {
        Ok(())
    }
}

impl<R, T> Read<R, ()> for PhantomData<T> {
    type Error = Infallible;

    #[inline]
    fn read(_reader: &mut R, _parameters: ()) -> Result<Self, Self::Error> {
        Ok(PhantomData)
    }
}

impl<R: ReadIntoBuf, const N: usize> Read<R, ()> for [u8; N] {
    type Error = <R as ReadIntoBuf>::Error;

    #[inline]
    fn read(reader: &mut R, _parameters: ()) -> Result<Self, Self::Error> {
        let mut buf = [0u8; N];
        reader.read_into_buf(&mut buf)?;
        Ok(buf)
    }
}

impl<R: ReadIntoBuf> Read<R, ()> for u8 {
    type Error = <R as ReadIntoBuf>::Error;

    #[inline]
    fn read(reader: &mut R, _parameters: ()) -> Result<Self, Self::Error> {
        let mut buf = [0u8; 1];
        reader.read_into_buf(&mut buf)?;
        Ok(buf[0])
    }
}

impl<R: ReadIntoBuf> Read<R, ()> for i8 {
    type Error = <R as ReadIntoBuf>::Error;

    #[inline]
    fn read(reader: &mut R, _parameters: ()) -> Result<Self, Self::Error> {
        let mut buf = [0u8; 1];
        reader.read_into_buf(&mut buf)?;
        Ok(buf[0] as i8)
    }
}

impl<R: ReadIntoBuf> Read<R, ()> for Ipv4Addr {
    type Error = <R as ReadIntoBuf>::Error;

    #[inline]
    fn read(reader: &mut R, _parameters: ()) -> Result<Self, Self::Error> {
        let mut buf = [0u8; 4];
        reader.read_into_buf(&mut buf)?;
        Ok(Ipv4Addr::from(buf))
    }
}

impl<R: ReadIntoBuf> Read<R, ()> for Ipv6Addr {
    type Error = <R as ReadIntoBuf>::Error;

    #[inline]
    fn read(reader: &mut R, _parameters: ()) -> Result<Self, Self::Error> {
        let mut buf = [0u8; 16];
        reader.read_into_buf(&mut buf)?;
        Ok(Ipv6Addr::from(buf))
    }
}

/// Implements [`Read`] for tuples.
///
/// # TODO
///
/// - add params
macro_rules! impl_read_for_tuple {
    (
        $index:tt => $name:ident: $ty:ident
    ) => {
        impl_read_for_tuple! {
            $index => $name: $ty,
        }
    };
    (
        $first_index:tt => $first_name:ident: $first_ty:ident,
        $($tail_index:tt => $tail_name:ident: $tail_ty:ident),*
    ) => {
        impl<R, $first_ty, $($tail_ty),*> Read<R, ()> for ($first_ty, $($tail_ty,)*)
        where

            $first_ty: Read<R, ()>,
            $($tail_ty: Read<R, (), Error = <$first_ty as Read<R, ()>>::Error>,)*
        {
            type Error = <$first_ty as Read<R, ()>>::Error;

            fn read(mut reader: &mut R, _parameters: ()) -> Result<Self, Self::Error> {
                let $first_name = <$first_ty as Read<R, ()>>::read(&mut reader, ())?;
                $(
                    let $tail_name = <$tail_ty as Read<R, ()>>::read(&mut reader, ())?;
                )*
                Ok((
                    $first_name,
                    $($tail_name,)*
                ))
            }
        }
    };
}
for_tuple!(impl_read_for_tuple! for 1..=8);

// for testing
/*impl<R, A, B> Read<R, ()> for (A, B)
where
    A: Read<R, ()>,
    B: Read<R, (), Error = <A as Read<R, ()>>::Error>,
{
    type Error = <A as Read<R, ()>>::Error;

    fn read(mut reader: &mut R, _parameters: ()) -> Result<Self, Self::Error> {
        let a = <A as Read<R, ()>>::read(&mut reader, ())?;
        let b = <B as Read<R, ()>>::read(&mut reader, ())?;
        Ok((a, b))
    }
}*/

/// Read macro
#[macro_export]
macro_rules! read {
    ($reader:expr => $ty:ty; $params:expr) => {
        {
            <$ty as ::byst::io::read::Read::<_, _>>::read($reader, $params)
        }
    };
    ($reader:expr => $ty:ty) => {
        read!($reader => $ty; ())
    };
    ($reader:expr; $params:expr) => {
        read!($reader => _; $params)
    };
    ($reader:expr) => {
        read!($reader => _; ())
    };
}
pub use read;
