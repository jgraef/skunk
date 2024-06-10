use std::{
    marker::PhantomData,
    ops::Bound,
};

use super::{
    buf::{
        NonEmptyIter,
        WriteError,
    },
    copy,
    endianness::{
        Decode,
        Size,
    },
    Buf,
    BufMut,
    CopyError,
    Endianness,
    NativeEndian,
    Range,
    RangeOutOfBounds,
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
            },
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

/// Trait for types that can be read.
///
/// This trait is used when by [`Reader::read_xe`] or [`WithXe::read`].
/// Todo: note that you need to avoid unbounded recursion.
pub trait Read<E: Endianness>: Sized {
    fn read<R: Reader>(reader: &mut R) -> Result<Self, End>;
}

/// implement `Read` for anything that can be decoded with
/// [`Decode<E>`][Decode].
impl<E: Endianness, T: Decode<E>> Read<E> for T
where
    [(); <Self as Size>::BYTES]: Sized,
{
    fn read<R: Reader>(reader: &mut R) -> Result<Self, End> {
        Ok(<T as Decode<E>>::decode(&reader.read_array()?))
    }
}

/// Something that can be read from.
///
/// This reader has no inherent endianness, so it must be specified when using
/// [`Self::read_xe`]. If you want a reader with fixed endianness, use a reader
/// that also implements [`WithXe`], such as [`ReaderXe`].
pub trait Reader: Sized {
    /// The slice type returned by [`Reader::read_slice`].
    type View<'a>: Buf + 'a
    where
        Self: 'a;

    /// Reads a view of length `n`.
    fn read_view(&mut self, n: usize) -> Result<Self::View<'_>, End>;

    /// Reads into `destination`, filling that buffer, but not growing it.
    fn read_into<D: BufMut>(&mut self, destination: D) -> Result<(), End>;

    /// Reads an array of length `N`.
    ///
    /// # Default implementation
    ///
    /// This has a default implementation that uses [`Self::read_into`],
    /// but you should override the implementation if there is a cheaper way of
    /// reading an array.
    #[inline]
    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], End> {
        let mut buf = [0u8; N];
        self.read_into(&mut buf)?;
        Ok(buf)
    }

    /// Reads the value with the given endianness.
    #[inline]
    fn read_xe<T: Read<E>, E: Endianness>(&mut self) -> Result<T, End> {
        <T as Read<E>>::read(self)
    }

    /// Skips `n` bytes.
    ///
    /// # Default implementation
    ///
    /// This has a default implementation that uses [`Self::skip`]. You should
    /// override this, if there is a cheaper way of skipping bytes.
    #[inline]
    fn skip(&mut self, n: usize) -> Result<(), End> {
        self.read_view(n)?;
        Ok(())
    }

    #[inline]
    fn read<T: Read<<Self as HasEndianness>::Endianness>>(&mut self) -> Result<T, End>
    where
        Self: HasEndianness,
    {
        self.read_xe::<T, <Self as HasEndianness>::Endianness>()
    }
}

pub trait Write<E: Endianness> {
    fn write<W: Writer>(writer: &mut W, value: &Self) -> Result<(), Full>;
}

pub trait Writer: Sized {
    fn write_buf<S: Buf>(&mut self, source: S) -> Result<(), Full>;

    #[inline]
    fn write_xe<T: Write<E>, E: Endianness>(&mut self, value: &T) -> Result<(), Full> {
        <T as Write<E>>::write(self, value)
    }

    #[inline]
    fn write<T: Write<<Self as HasEndianness>::Endianness>>(
        &mut self,
        value: &T,
    ) -> Result<(), Full>
    where
        Self: HasEndianness,
    {
        self.write_xe::<T, <Self as HasEndianness>::Endianness>(value)
    }
}

/// Trait for readers and writers that have an inherent endianness.
///
/// An useful implementor of this trait is [`WithXe`], which is a wrapper that
/// gives any [`Reader`] or [`Writer`] an inherent endianness.
pub trait HasEndianness {
    type Endianness: Endianness;
}

/// Wrapper around reader that gives it an inherent endianness.
pub struct WithXe<T, E: Endianness> {
    inner: T,
    _endianness: PhantomData<E>,
}

impl<T, E: Endianness> WithXe<T, E> {
    #[inline]
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            _endianness: PhantomData,
        }
    }

    #[inline]
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T, E: Endianness> From<T> for WithXe<T, E> {
    #[inline]
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<R: Reader, E: Endianness> Reader for WithXe<R, E> {
    type View<'a> = R::View<'a>
    where
        Self: 'a;

    #[inline]
    fn read_view(&mut self, n: usize) -> Result<Self::View<'_>, End> {
        self.inner.read_view(n)
    }

    #[inline]
    fn read_into<D: BufMut>(&mut self, destination: D) -> Result<(), End> {
        self.inner.read_into(destination)
    }

    #[inline]
    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], End> {
        self.inner.read_array()
    }

    #[inline]
    fn read_xe<T: Read<E2>, E2: Endianness>(&mut self) -> Result<T, End> {
        self.inner.read_xe::<T, E2>()
    }

    #[inline]
    fn skip(&mut self, n: usize) -> Result<(), End> {
        self.inner.skip(n)
    }
}

impl<W: Writer, E: Endianness> Writer for WithXe<W, E> {
    #[inline]
    fn write_buf<S: Buf>(&mut self, source: S) -> Result<(), Full> {
        self.inner.write_buf(source)
    }

    #[inline]
    fn write_xe<T: Write<E2>, E2: Endianness>(&mut self, value: &T) -> Result<(), Full> {
        self.inner.write_xe::<T, E2>(value)
    }
}

impl<T, E: Endianness> HasEndianness for WithXe<T, E> {
    type Endianness = E;
}

impl<T, E: Endianness> AsRef<T> for WithXe<T, E> {
    fn as_ref(&self) -> &T {
        &self.inner
    }
}

impl<T, E: Endianness> AsMut<T> for WithXe<T, E> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.inner
    }
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

/// A [`Reader`] and [`Writer`] that reads and writes from and to a [`Buf`].
///
/// This reader/writer has an inherent endianness of [`NativeEndian`]. You can
/// use [`WithXe`] to change the endianness.
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
        Range {
            start: Bound::Included(self.offset),
            end: Bound::Excluded(self.offset + n),
        }
    }
}

impl<B: Buf> Reader for Cursor<B> {
    type View<'a> = <B as Buf>::View<'a> where Self: 'a;

    fn read_view(&mut self, n: usize) -> Result<Self::View<'_>, End> {
        let range = self.get_range(n);
        let output = self.buf.view(range).unwrap();
        self.offset += n;
        Ok(output)
    }

    fn read_into<D: BufMut>(&mut self, destination: D) -> Result<(), End> {
        let n = destination.len();
        let range = self.get_range(n);
        copy(destination, .., &self.buf, range).map_err(End::from_copy_error)?;
        self.offset += n;
        Ok(())
    }

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

impl<B: BufMut> Writer for Cursor<B> {
    fn write_buf<S: Buf>(&mut self, source: S) -> Result<(), Full> {
        let n = source.len();
        let range = self.get_range(n);
        self.buf
            .write(range, source, ..)
            .map_err(Full::from_write_error)?;
        self.offset += n;
        Ok(())
    }
}

impl<B: Buf> HasEndianness for Cursor<B> {
    type Endianness = NativeEndian;
}

//impl<B: BufMUt> Writer

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

// todo: implement this. or do we even need this? don't forget to make this pub.
struct ChunksReader<'a, I: Iterator<Item = &'a [u8]>> {
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
