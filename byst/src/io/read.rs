use std::{
    convert::Infallible,
    marker::PhantomData,
    net::{
        Ipv4Addr,
        Ipv6Addr,
    },
};

use byst_macros::for_tuple;

use crate::{
    impl_me,
    Buf,
    BufMut,
    RangeOutOfBounds,
};

/// Something that can be read from a reader `R`, given the context `C`.
pub trait Read<R: ?Sized, C>: Sized {
    type Error;

    fn read(reader: &mut R, context: C) -> Result<Self, Self::Error>;
}

pub trait Reader {
    // todo: remove `limit` argument. `dest` could be an `impl Writer`
    fn read_into<D: BufMut>(&mut self, dest: D, limit: impl Into<Option<usize>>) -> usize;

    fn skip(&mut self, amount: usize) -> usize;
}

pub trait ReaderExt: Reader {
    #[inline]
    fn read<T: Read<Self, ()>>(&mut self) -> Result<T, T::Error> {
        self.read_with(())
    }

    #[inline]
    fn read_with<T: Read<Self, C>, C>(&mut self, context: C) -> Result<T, T::Error> {
        T::read(self, context)
    }

    #[inline]
    fn read_byte_array<const N: usize>(&mut self) -> Result<[u8; N], End> {
        let mut buf = [0u8; N];
        if self.read_into(&mut buf, None) == N {
            Ok(buf)
        }
        else {
            Err(End)
        }
    }

    #[inline]
    fn limit(&mut self, limit: usize) -> Limit<&mut Self> {
        Limit::new(self, limit)
    }
}

impl<R: Reader> ReaderExt for R {}

pub trait BufReader: Reader {
    type View: Buf;

    fn view(&self, length: usize) -> Result<Self::View, End>;

    fn chunk(&self) -> Result<&[u8], End>;

    fn advance(&mut self, by: usize) -> Result<(), End>;

    fn remaining(&self) -> usize;

    fn rest(&mut self) -> Self::View;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, thiserror::Error)]
#[error("End of reader")]
pub struct End;

impl End {
    #[inline]
    pub(crate) fn from_range_out_of_bounds(_: RangeOutOfBounds) -> Self {
        // todo: we could do some checks here, if it's really an error that can be
        // interpreted as end of buffer.
        Self
    }
}

impl From<End> for std::io::ErrorKind {
    #[inline]
    fn from(_: End) -> Self {
        std::io::ErrorKind::UnexpectedEof
    }
}

impl From<End> for std::io::Error {
    #[inline]
    fn from(_: End) -> Self {
        std::io::ErrorKind::UnexpectedEof.into()
    }
}

impl From<Infallible> for End {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, thiserror::Error)]
#[error("Invalid discriminant: {0}")]
pub struct InvalidDiscriminant<D>(pub D);

impl<'a, R: Reader> Reader for &'a mut R {
    #[inline]
    fn read_into<D: BufMut>(&mut self, dest: D, limit: impl Into<Option<usize>>) -> usize {
        Reader::read_into(*self, dest, limit)
    }

    #[inline]
    fn skip(&mut self, amount: usize) -> usize {
        Reader::skip(*self, amount)
    }
}

impl_me! {
    impl['a] Reader for &'a [u8] as BufReader;
    impl['a] Read<Self, ()> for &'a [u8] as BufReader;
}

impl<'a, R: BufReader> BufReader for &'a mut R {
    type View = R::View;

    #[inline]
    fn view(&self, length: usize) -> Result<Self::View, End> {
        R::view(self, length)
    }

    #[inline]
    fn chunk(&self) -> Result<&[u8], End> {
        R::chunk(self)
    }

    #[inline]
    fn advance(&mut self, by: usize) -> Result<(), End> {
        R::advance(self, by)
    }

    #[inline]
    fn remaining(&self) -> usize {
        R::remaining(self)
    }

    #[inline]
    fn rest(&mut self) -> Self::View {
        R::rest(self)
    }
}

impl<'a> BufReader for &'a [u8] {
    type View = &'a [u8];

    #[inline]
    fn view(&self, length: usize) -> Result<Self::View, End> {
        (length <= self.len()).then_some(&self[..length]).ok_or(End)
    }

    #[inline]
    fn chunk(&self) -> Result<&'a [u8], End> {
        (!self.is_empty()).then_some(&self[..]).ok_or(End)
    }

    #[inline]
    fn advance(&mut self, by: usize) -> Result<(), End> {
        (by <= self.len())
            .then(|| {
                *self = &self[by..];
            })
            .ok_or(End)
    }

    #[inline]
    fn remaining(&self) -> usize {
        self.len()
    }

    #[inline]
    fn rest(&mut self) -> Self::View {
        std::mem::take(self)
    }
}

impl<R> Read<R, ()> for () {
    type Error = Infallible;

    #[inline]
    fn read(_reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        Ok(())
    }
}

impl<R, T> Read<R, ()> for PhantomData<T> {
    type Error = Infallible;

    #[inline]
    fn read(_reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        Ok(PhantomData)
    }
}
/*
impl<R: Reader, C, T: Read<R, C>, const N: usize> Read<R, C> for [T; N] {
    type Error = End;

    #[inline]
    fn read(reader: &mut R, _context: C) -> Result<Self, Self::Error> {
        todo!();
    }
}
*/

impl<R: Reader, const N: usize> Read<R, ()> for [u8; N] {
    type Error = End;

    #[inline]
    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        reader.read_byte_array()
    }
}

impl<R: Reader> Read<R, ()> for u8 {
    type Error = End;

    #[inline]
    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        Ok(reader.read_byte_array::<1>()?[0])
    }
}

impl<R: Reader> Read<R, ()> for i8 {
    type Error = End;

    #[inline]
    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        Ok(reader.read::<u8>()? as i8)
    }
}

impl<R: Reader> Read<R, ()> for Ipv4Addr {
    type Error = End;

    #[inline]
    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        Ok(Ipv4Addr::from(reader.read::<[u8; 4]>()?))
    }
}

impl<R: Reader> Read<R, ()> for Ipv6Addr {
    type Error = End;

    #[inline]
    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        Ok(Ipv6Addr::from(reader.read::<[u8; 16]>()?))
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

            fn read(mut reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
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

/// Read macro
///
/// # TODO
///
/// - deprecate this. `reader.read::<T>()` and `reader.read_with::<T,
///   _>(context)` are nicer.
#[macro_export]
macro_rules! read {
    ($reader:expr => $ty:ty; $params:expr) => {
        {
            <$ty as ::byst::io::Read::<_, _>>::read($reader, $params)
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

use super::Limit;
