use super::{
    buf::NonEmptyIter,
    copy,
    BigEndian,
    Buf,
    CopyError,
    Endianness,
    LittleEndian,
    NativeEndian,
    NetworkEndian,
    RangeOutOfBounds,
};
use crate::util::Peekable;

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("End of reader")]
pub struct End;

impl End {
    fn from_copy_error(e: CopyError) -> Self {
        match e {
            CopyError::SourceRangeOutOfBounds(_) => Self,
            _ => panic!("Unexpected error while copying: {e}"),
        }
    }

    fn from_range_out_of_bounds(_: RangeOutOfBounds) -> Self {
        // todo: we could do some checks here, if it's really an error that can be
        // interpreted as end of buffer.
        End
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

/// Something that can be read from.
pub trait Reader: Sized {
    /// The slice type returned by [`Reader::read_slice`].
    type View<'a>: Buf + 'a
    where
        Self: 'a;

    /// Reads a view of length `n`.
    fn read_view(&mut self, n: usize) -> Result<Self::View<'_>, End>;

    /// Reads an array of length `N`.
    ///
    /// # Default implementation
    ///
    /// This has a default implementation that uses [`Self::read_view`],
    /// but you should override the implementation if there is a cheaper way of
    /// reading an array.
    #[inline]
    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], End> {
        let view = self.read_view(N)?;
        let mut buf = [0u8; N];
        copy(&mut buf, .., view, ..).map_err(End::from_copy_error)?;
        Ok(buf)
    }

    /// Reads the value with the given endianess.
    #[inline]
    fn read_xe<T: Read<E>, E: Endianness>(&mut self) -> Result<T, End> {
        <T as Read<E>>::read(self)
    }

    /// Reads the value in native byte order.
    #[inline]
    fn read<T: Read<NativeEndian>>(&mut self) -> Result<T, End> {
        self.read_xe::<T, NativeEndian>()
    }

    /// Reads the value with big-endian byte order.
    #[inline]
    fn read_be<T: Read<BigEndian>>(&mut self) -> Result<T, End> {
        self.read_xe::<T, BigEndian>()
    }

    /// Reads the value with little-endian byte order.
    #[inline]
    fn read_le<T: Read<LittleEndian>>(&mut self) -> Result<T, End> {
        self.read_xe::<T, LittleEndian>()
    }

    /// Reads the value with network-endian byte order.
    #[inline]
    fn read_ne<T: Read<NetworkEndian>>(&mut self) -> Result<T, End> {
        self.read_xe::<T, NetworkEndian>()
    }

    #[inline]
    fn skip(&mut self, n: usize) -> Result<(), End> {
        self.read_view(n)?;
        Ok(())
    }
}

pub trait Read<E: Endianness>: Sized {
    fn read<R: Reader>(reader: &mut R) -> Result<Self, End>;
}

macro_rules! impl_read_for_ints {
    {
        $(
            $ty:ty => $method:ident;
        )*
    } => {
        $(
            impl<E: Endianness> Read<E> for $ty {
                #[inline]
                fn read<R: Reader>(reader: &mut R) -> Result<Self, End> {
                    Ok(E::$method(reader.read_array()?))
                }
            }
        )*
    };
}

impl_read_for_ints! {
    u8 => u8_from_bytes;
    i8 => i8_from_bytes;
    u16 => u16_from_bytes;
    i16 => i16_from_bytes;
    u32 => u32_from_bytes;
    i32 => i32_from_bytes;
    u64 => u64_from_bytes;
    i64 => i64_from_bytes;
    u128 => u128_from_bytes;
    i128 => i128_from_bytes;
    f32 => f32_from_bytes;
    f64 => f64_from_bytes;
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

#[derive(Clone, Debug)]
pub struct BufReader<B> {
    buf: B,
    offset: usize,
}

impl<B> BufReader<B> {
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

impl<B: Buf> Reader for BufReader<B> {
    type View<'a> = <B as Buf>::View<'a> where Self: 'a;

    #[inline]
    fn read_view(&mut self, n: usize) -> Result<Self::View<'_>, End> {
        let output = self
            .buf
            .view(self.offset..(self.offset + n))
            .map_err(End::from_range_out_of_bounds)?;
        self.offset += n;
        Ok(output)
    }

    #[inline]
    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], End> {
        let mut buf = [0u8; N];
        copy(&mut buf, .., &self.buf, self.offset..(self.offset + N))
            .map_err(End::from_copy_error)?;
        self.offset += N;
        Ok(buf)
    }
}

impl<B: Buf> Remaining for BufReader<B> {
    #[inline]
    fn remaining(&self) -> usize {
        self.buf.len() - self.offset
    }
}

impl<B: Buf> Position for BufReader<B> {
    #[inline]
    fn position(&self) -> usize {
        self.offset
    }

    #[inline]
    fn set_position(&mut self, position: usize) {
        self.offset = position;
    }
}

impl<B> From<B> for BufReader<B> {
    #[inline]
    fn from(value: B) -> Self {
        Self::new(value)
    }
}

// todo: implement this. or do we even need this?
struct ChunksReader<'a, I: Iterator<Item = &'a [u8]>> {
    inner: Peekable<NonEmptyIter<I>>,
}

impl<'a, I: Iterator<Item = &'a [u8]>> ChunksReader<'a, I> {
    pub fn new(inner: I) -> Self {
        Self {
            inner: Peekable::new(NonEmptyIter(inner)),
        }
    }

    pub fn into_parts(self) -> (I, Option<&'a [u8]>) {
        let (iter, peeked) = self.inner.into_parts();
        (iter.0, peeked)
    }
}
