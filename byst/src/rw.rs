use std::marker::PhantomData;

use byst_macros::for_tuple;
pub use byst_macros::{
    Read,
    Write,
};

use super::{
    buf::{
        chunks::NonEmptyIter,
        Buf,
        BufMut,
        WriteError,
    },
    copy::{
        copy,
        CopyError,
    },
    range::{
        Range,
        RangeOutOfBounds,
    },
};
use crate::util::Peekable;

#[derive(Clone, Copy, Debug, Default, thiserror::Error)]
#[error("End of reader")]
pub struct End;

impl End {
    fn from_copy_error(e: CopyError) -> Self {
        match e {
            CopyError::SourceRangeOutOfBounds(_) => Self,
            _ => {
                panic!("Unexpected error while copying: {e}");
            }
        }
    }

    fn from_range_out_of_bounds(_: RangeOutOfBounds) -> Self {
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

#[derive(Clone, Copy, Debug, Default, thiserror::Error)]
#[error("Writer is full")]
pub struct Full;

impl Full {
    fn from_write_error(e: WriteError) -> Self {
        match e {
            WriteError::Full { .. } => Full,
            _ => panic!("Unexpected error while writing: {e}"),
        }
    }
}

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

/// A reader that knows how many bytes are remaining.
pub trait Remaining {
    fn remaining(&self) -> usize;

    #[inline]
    fn is_at_end(&self) -> bool {
        self.remaining() == 0
    }
}

/// A reader that also has knowledge about the position in the underlying
/// buffer.
pub trait Position {
    fn position(&self) -> usize;

    /// Set the position of the reader.
    ///
    /// It is up to the implementor how to handle invalid `position`s. The
    /// options are:
    ///
    /// 1. Panic immediately when [`set_position`](Self::set_position) is
    ///    called.
    /// 2. Ignore invalid positions until the [`Reader`] is being read from, and
    ///    then return [`End`].
    fn set_position(&mut self, position: usize);

    #[inline]
    fn is_at_start(&self) -> bool {
        self.position() == 0
    }

    #[inline]
    fn reset_position(&mut self) {
        self.set_position(0);
    }
}

/// A reader or writer that can skip bytes.
pub trait Skip {
    fn skip(&mut self, n: usize) -> Result<(), End>;
}

impl<'r, R: ReadIntoBuf> ReadIntoBuf for &'r mut R {
    #[inline]
    fn read_into_buf<B: BufMut>(&mut self, buf: B) -> Result<(), End> {
        (*self).read_into_buf(buf)
    }
}

impl<'w, W: WriteFromBuf> WriteFromBuf for &'w mut W {
    #[inline]
    fn write_from_buf<B: Buf>(&mut self, buf: B) -> Result<(), Full> {
        (*self).write_from_buf(buf)
    }
}

impl<R> Read<R> for () {
    #[inline]
    fn read(_reader: R) -> Result<Self, End> {
        Ok(())
    }
}

impl<W> Write<W> for () {
    #[inline]
    fn write(&self, _writer: W) -> Result<(), Full> {
        Ok(())
    }
}

impl<R, T> Read<R> for PhantomData<T> {
    #[inline]
    fn read(_reader: R) -> Result<Self, End> {
        Ok(PhantomData)
    }
}

impl<W, T> Write<W> for PhantomData<T> {
    #[inline]
    fn write(&self, _writer: W) -> Result<(), Full> {
        Ok(())
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

impl<W: WriteFromBuf, const N: usize> Write<W> for [u8; N] {
    #[inline]
    fn write(&self, mut writer: W) -> Result<(), Full> {
        writer.write_from_buf(self)
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

impl<W: WriteFromBuf> Write<W> for u8 {
    #[inline]
    fn write(&self, mut writer: W) -> Result<(), Full> {
        let buf = [*self];
        writer.write_from_buf(&buf)
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

impl<W: WriteFromBuf> Write<W> for i8 {
    #[inline]
    fn write(&self, mut writer: W) -> Result<(), Full> {
        let buf = [*self as u8];
        writer.write_from_buf(&buf)
    }
}

// implement `Read` and `Write` for tuples.
// todo: also implement `ReadXe` and `WriteXe`.
macro_rules! impl_tuple {
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
for_tuple!(impl_tuple! for 1..=8);

/// Read macro
#[macro_export]
macro_rules! read {
    ($reader:ident => $ty:ty as $endianness:ty) => {
        {
            <$ty as ::byst::rw::ReadXe::<_, $endianness>>::read(&mut $reader)
        }
    };
    ($reader:ident => $ty:ty) => {
        {
            <$ty as ::byst::rw::Read::<_>>::read(&mut $reader)
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

/// A reader and writer that reads and writes from and to a [`Buf`].
#[derive(Clone, Debug)]
pub struct Cursor<B> {
    buf: B,
    offset: usize,
}

impl<B> Cursor<B> {
    #[inline]
    pub fn new(buf: B) -> Self {
        Self::with_offset(buf, 0)
    }

    #[inline]
    pub fn with_offset(buf: B, offset: usize) -> Self {
        Self { buf, offset }
    }

    #[inline]
    pub fn into_inner(self) -> B {
        self.buf
    }
}

impl<B: Buf> Cursor<B> {
    #[inline]
    fn get_range(&self, n: usize) -> Range {
        Range::default().with_start(self.offset).with_length(n)
    }
}

impl<B: Buf> ReadIntoBuf for Cursor<B> {
    fn read_into_buf<D: BufMut>(&mut self, buf: D) -> Result<(), End> {
        let n = buf.len();
        let range = self.get_range(n);
        copy(buf, .., &self.buf, range).map_err(End::from_copy_error)?;
        self.offset += n;
        Ok(())
    }
}

impl<B: BufMut> WriteFromBuf for Cursor<B> {
    fn write_from_buf<S: Buf>(&mut self, source: S) -> Result<(), Full> {
        let n = source.len();
        let range = self.get_range(n);
        self.buf
            .write(range, source, ..)
            .map_err(Full::from_write_error)?;
        self.offset += n;
        Ok(())
    }
}

/// Wrapper type for reading views.
#[derive(
    Clone,
    Copy,
    Debug,
    derive_more::From,
    derive_more::Deref,
    derive_more::DerefMut,
    derive_more::AsRef,
    derive_more::AsMut,
)]
pub struct View<B: Buf>(pub B);

impl<'b, B: Buf<View<'b> = V> + 'b, V: Buf> Read<&'b mut Cursor<B>> for View<V> {
    fn read(reader: &'b mut Cursor<B>) -> Result<Self, End> {
        let range = Range::default().with_start(reader.offset);
        let view = reader
            .buf
            .view(range)
            .map_err(End::from_range_out_of_bounds)?;
        reader.offset += view.len();
        Ok(View(view))
    }
}

impl<B: Buf> Skip for Cursor<B> {
    fn skip(&mut self, n: usize) -> Result<(), End> {
        let range = self.get_range(n);
        if self.buf.contains(range) {
            self.offset += n;
            Ok(())
        }
        else {
            Err(End)
        }
    }
}

impl<B> AsRef<B> for Cursor<B> {
    #[inline]
    fn as_ref(&self) -> &B {
        &self.buf
    }
}

impl<B> AsMut<B> for Cursor<B> {
    #[inline]
    fn as_mut(&mut self) -> &mut B {
        &mut self.buf
    }
}

impl<B: Buf> Remaining for Cursor<B> {
    #[inline]
    fn remaining(&self) -> usize {
        self.buf.len() - self.offset
    }
}

impl<B: Buf> Position for Cursor<B> {
    #[inline]
    fn position(&self) -> usize {
        self.offset
    }

    #[inline]
    fn set_position(&mut self, position: usize) {
        self.offset = position;
    }
}

impl<B> From<B> for Cursor<B> {
    #[inline]
    fn from(value: B) -> Self {
        Self::new(value)
    }
}

#[allow(dead_code)]
mod todo {
    use super::*;
    // todo: implement this. or do we even need this? don't forget to make this pub.

    pub struct ChunksReader<'a, I: Iterator<Item = &'a [u8]>> {
        inner: Peekable<NonEmptyIter<I>>,
    }

    impl<'a, I: Iterator<Item = &'a [u8]>> ChunksReader<'a, I> {
        #[inline]
        pub fn new(inner: I) -> Self {
            Self {
                inner: Peekable::new(NonEmptyIter(inner)),
            }
        }

        #[inline]
        pub fn into_parts(self) -> (I, Option<&'a [u8]>) {
            let (iter, peeked) = self.inner.into_parts();
            (iter.0, peeked)
        }
    }
}
