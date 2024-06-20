use std::marker::PhantomData;

use byst_macros::for_tuple;

use crate::buf::Buf;

#[derive(Clone, Copy, Debug, Default, thiserror::Error)]
#[error("Writer is full")]
pub struct Full;

/// Something that can be written without specifying endianness.
pub trait Write<W> {
    fn write(&self, writer: W) -> Result<(), Full>;
}

/// Something that can be written with endianness.
pub trait WriteXe<W, E> {
    fn write(&self, writer: W) -> Result<(), Full>;
}

/// A writer that can write bytes from a buffer.
///
/// Implementing this will cause [`Write`] and [`WriteXe`] to be implemented for
/// many common types.
pub trait WriteFromBuf {
    fn write_from_buf<B: Buf>(&mut self, buf: B) -> Result<(), Full>;
}

impl<'w, W: WriteFromBuf> WriteFromBuf for &'w mut W {
    #[inline]
    fn write_from_buf<B: Buf>(&mut self, buf: B) -> Result<(), Full> {
        (*self).write_from_buf(buf)
    }
}

impl<W> Write<W> for () {
    #[inline]
    fn write(&self, _writer: W) -> Result<(), Full> {
        Ok(())
    }
}

impl<W, T> Write<W> for PhantomData<T> {
    #[inline]
    fn write(&self, _writer: W) -> Result<(), Full> {
        Ok(())
    }
}

impl<W: WriteFromBuf, const N: usize> Write<W> for [u8; N] {
    #[inline]
    fn write(&self, mut writer: W) -> Result<(), Full> {
        writer.write_from_buf(self)
    }
}

impl<W: WriteFromBuf> Write<W> for u8 {
    #[inline]
    fn write(&self, mut writer: W) -> Result<(), Full> {
        let buf = [*self];
        writer.write_from_buf(&buf)
    }
}

impl<W: WriteFromBuf> Write<W> for i8 {
    #[inline]
    fn write(&self, mut writer: W) -> Result<(), Full> {
        let buf = [*self as u8];
        writer.write_from_buf(&buf)
    }
}

macro_rules! impl_write_for_tuple {
    ($($index:tt => $name:ident: $ty:ident),*) => {
        impl<W, $($ty),*> Write<W> for ($($ty,)*)
        where
            $($ty: for<'w> Write<&'w mut W>,)*
        {
            fn write(&self, mut writer: W) -> Result<(), Full> {
                $(
                    let $name = &self.$index;
                )*
                $(
                    <$ty as Write<&mut W>>::write($name, &mut writer)?;
                )*
                Ok(())
            }
        }
    };
}
for_tuple!(impl_write_for_tuple! for 1..=8);
