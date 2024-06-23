use std::{
    convert::Infallible,
    marker::PhantomData,
};

use byst_macros::for_tuple;

use super::{
    End,
    Limit,
};
use crate::{
    buf::Buf,
    impl_me,
};

/// Something that can be written to a writer `W`, given the context `C`.
pub trait Write<W: ?Sized, C> {
    type Error;

    fn write(&self, writer: &mut W, context: C) -> Result<(), Self::Error>;
}

pub trait Writer {
    type Error;

    fn write_buf<B: Buf>(&mut self, buf: B) -> Result<(), Full>;
    fn skip(&mut self, amount: usize) -> Result<(), Full>;
}

pub trait WriterExt: Writer {
    #[inline]
    fn write<T: Write<Self, ()>>(&mut self, value: &T) -> Result<(), T::Error> {
        Self::write_with(self, value, ())
    }

    #[inline]
    fn write_with<T: Write<Self, C>, C>(&mut self, value: &T, context: C) -> Result<(), T::Error> {
        T::write(value, self, context)
    }

    #[inline]
    fn limit(&mut self, limit: usize) -> Limit<&mut Self> {
        Limit::new(self, limit)
    }
}

impl<W: Writer> WriterExt for W {}

pub trait BufWriter: Writer {
    fn chunk_mut(&mut self) -> Result<&mut [u8], End>;

    fn advance(&mut self, by: usize) -> Result<(), Full>;

    fn remaining(&self) -> usize;

    fn extend(&mut self, with: &[u8]) -> Result<(), Full>;
}

#[derive(Clone, Copy, Debug, Default, thiserror::Error)]
#[error(
    "Writer full: Tried to write {requested} bytes, but only {written} bytes could be written."
)]
pub struct Full {
    pub written: usize,
    pub requested: usize,
    pub remaining: usize,
}

impl From<crate::buf::Full> for Full {
    fn from(value: crate::buf::Full) -> Self {
        Self {
            written: 0,
            requested: value.required,
            remaining: value.capacity,
        }
    }
}

impl<'w, W: Writer> Writer for &'w mut W {
    type Error = W::Error;

    #[inline]
    fn write_buf<B: Buf>(&mut self, buf: B) -> Result<(), Full> {
        <W as Writer>::write_buf(*self, buf)
    }

    #[inline]
    fn skip(&mut self, amount: usize) -> Result<(), Full> {
        <W as Writer>::skip(*self, amount)
    }
}

impl_me! {
    impl['a] Writer for &'a mut [u8] as BufWriter;
    impl['a] Write<_, ()> for &'a [u8] as Writer::write_buf;
    //impl['a] Writer for &'a mut Vec<u8> as BufWriter;
}

impl<'a, W: BufWriter> BufWriter for &'a mut W {
    fn chunk_mut(&mut self) -> Result<&mut [u8], End> {
        W::chunk_mut(self)
    }

    fn advance(&mut self, by: usize) -> Result<(), Full> {
        W::advance(self, by)
    }

    fn remaining(&self) -> usize {
        W::remaining(self)
    }

    fn extend(&mut self, with: &[u8]) -> Result<(), Full> {
        W::extend(self, with)
    }
}

impl<'a> BufWriter for &'a mut [u8] {
    #[inline]
    fn chunk_mut(&mut self) -> Result<&mut [u8], End> {
        (!self.is_empty()).then_some(&mut **self).ok_or(End)
    }

    #[inline]
    fn advance(&mut self, by: usize) -> Result<(), Full> {
        if by <= self.len() {
            let (_, rest) = std::mem::take(self).split_at_mut(by);
            *self = rest;
            Ok(())
        }
        else {
            Err(Full {
                requested: by,
                remaining: self.len(),
                written: 0,
            })
        }
    }

    #[inline]
    fn remaining(&self) -> usize {
        self.len()
    }

    #[inline]
    fn extend(&mut self, with: &[u8]) -> Result<(), Full> {
        if with.len() <= self.len() {
            let (dest, rest) = std::mem::take(self).split_at_mut(with.len());
            dest.copy_from_slice(with);
            *self = rest;
            Ok(())
        }
        else {
            Err(Full {
                requested: with.len(),
                remaining: self.len(),
                written: 0,
            })
        }
    }
}

impl<W> Write<W, ()> for () {
    type Error = Infallible;

    #[inline]
    fn write(&self, _writer: &mut W, _context: ()) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<W, T> Write<W, ()> for PhantomData<T> {
    type Error = Infallible;

    #[inline]
    fn write(&self, _writer: &mut W, _context: ()) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<W: Writer, const N: usize> Write<W, ()> for [u8; N] {
    type Error = Full;

    #[inline]
    fn write(&self, writer: &mut W, _context: ()) -> Result<(), Self::Error> {
        writer.write_buf(self)
    }
}

impl<W: Writer> Write<W, ()> for u8 {
    type Error = Full;

    #[inline]
    fn write(&self, writer: &mut W, _context: ()) -> Result<(), Self::Error> {
        writer.write_buf(&[*self])
    }
}

impl<W: Writer> Write<W, ()> for i8 {
    type Error = Full;

    #[inline]
    fn write(&self, writer: &mut W, _context: ()) -> Result<(), Self::Error> {
        writer.write(&(*self as u8))
    }
}

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
        impl<W, $first_ty, $($tail_ty),*> Write<W, ()> for ($first_ty, $($tail_ty,)*)
        where
            $first_ty: Write<W, ()>,
            $($tail_ty: Write<W, (), Error = <$first_ty as Write<W, ()>>::Error>,)*
        {
            type Error = <$first_ty as Write<W, ()>>::Error;

            fn write(&self, mut writer: &mut W, _context: ()) -> Result<(), Self::Error> {
                <$first_ty as Write<W, ()>>::write(&self.$first_index, &mut writer, ())?;
                $(
                    <$tail_ty as Write<W, ()>>::write(&self.$tail_index, &mut writer, ())?;
                )*
                Ok(())
            }
        }
    };
}
for_tuple!(impl_read_for_tuple! for 1..=8);
