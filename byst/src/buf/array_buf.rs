use std::{
    fmt::Debug,
    mem::MaybeUninit,
};

use super::{
    partially_initialized::{
        PartiallyInitialized,
        PartiallyInitializedWriter,
    },
    BufWriter,
    Full,
    Length,
};
use crate::{
    buf::{
        Buf,
        BufMut,
        SizeLimit,
    },
    io::{
        End,
        Writer,
    },
    range::{
        Range,
        RangeOutOfBounds,
    },
    util::buf_eq,
};

/// A buffer backed by an array. The array is initially empty, but can grow
/// until it reaches its capacity `N`.
pub struct ArrayBuf<const N: usize> {
    inner: PartiallyInitialized<[MaybeUninit<u8>; N]>,
}

impl<const N: usize> ArrayBuf<N> {
    #[inline]
    pub fn new() -> Self {
        Self {
            inner: PartiallyInitialized::new([MaybeUninit::uninit(); N]),
        }
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.inner.is_full()
    }

    pub fn resize(&mut self, new_len: usize, value: u8) {
        self.inner.resize(new_len, value)
    }

    #[inline]
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    #[inline]
    pub fn inner_ref(&self) -> &[MaybeUninit<u8>; N] {
        self.inner.inner_ref()
    }

    #[inline]
    pub fn inner_mut(&mut self) -> &mut [MaybeUninit<u8>; N] {
        self.inner.inner_mut()
    }
}

impl<const N: usize> Default for ArrayBuf<N> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> AsRef<[u8]> for ArrayBuf<N> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.inner.as_ref()
    }
}

impl<const N: usize> AsMut<[u8]> for ArrayBuf<N> {
    #[inline]
    fn as_mut(&mut self) -> &mut [u8] {
        self.inner.as_mut()
    }
}

impl<const N: usize> Clone for ArrayBuf<N> {
    #[inline]
    fn clone(&self) -> Self {
        // note: we could auto-derive this, since `MaybeUninit` implements `Clone` if
        // `T` (i.e. `u8`) is `Copy`. But it would copy the whole buffer, and we only
        // copy the portion that has been initialized.

        let mut cloned = Self::new();
        let source = self.inner.as_ref();

        MaybeUninit::copy_from_slice(&mut cloned.inner.inner_mut()[..source.len()], source);
        unsafe {
            // SAFETY: We just initialized the first `source.len()` bytes
            cloned.inner.assume_initialized(source.len());
        }

        cloned
    }
}

impl<const N: usize> Copy for ArrayBuf<N> {}

impl<const N: usize> Debug for ArrayBuf<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.inner, f)
    }
}

impl<const N: usize, T: Buf> PartialEq<T> for ArrayBuf<N> {
    #[inline]
    fn eq(&self, other: &T) -> bool {
        buf_eq(self, other)
    }
}

impl<const N: usize> Eq for ArrayBuf<N> {}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Buffer only partially initialized: {initialized} < {buf_size}")]
pub struct NotFullyInitialized {
    pub initialized: usize,
    pub buf_size: usize,
}

impl<const N: usize> TryFrom<ArrayBuf<N>> for [u8; N] {
    type Error = NotFullyInitialized;

    fn try_from(value: ArrayBuf<N>) -> Result<Self, Self::Error> {
        let (buf, initialized) = value.inner.into_parts();
        if initialized == N {
            unsafe {
                // SAFETY: we just checked that it's fully initialized.
                Ok(MaybeUninit::array_assume_init(buf))
            }
        }
        else {
            Err(NotFullyInitialized {
                initialized,
                buf_size: N,
            })
        }
    }
}

impl<const N: usize> Buf for ArrayBuf<N> {
    type View<'a> = &'a [u8]
    where
        Self: 'a;

    type Reader<'a> = &'a [u8]
    where
        Self: 'a;

    #[inline]
    fn view(&self, range: impl Into<Range>) -> Result<Self::View<'_>, RangeOutOfBounds> {
        self.inner.view(range)
    }

    #[inline]
    fn reader(&self) -> Self::Reader<'_> {
        self.inner.reader()
    }
}

impl<const N: usize> Length for ArrayBuf<N> {
    #[inline]
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<const N: usize> BufMut for ArrayBuf<N> {
    type ViewMut<'a> = &'a mut [u8]
    where
        Self: 'a;

    type Writer<'a> = ArrayBufWriter<'a, N>
    where
        Self: 'a;

    #[inline]
    fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds> {
        self.inner.view_mut(range)
    }

    #[inline]
    fn writer(&mut self) -> Self::Writer<'_> {
        ArrayBufWriter {
            inner: self.inner.writer(),
        }
    }

    #[inline]
    fn reserve(&mut self, size: usize) -> Result<(), Full> {
        self.inner.reserve(size)
    }

    #[inline]
    fn size_limit(&self) -> SizeLimit {
        N.into()
    }
}

pub struct ArrayBufWriter<'a, const N: usize> {
    inner: PartiallyInitializedWriter<'a, [MaybeUninit<u8>; N]>,
}

impl<'a, const N: usize> BufWriter for ArrayBufWriter<'a, N> {
    #[inline]
    fn chunk_mut(&mut self) -> Result<&mut [u8], End> {
        self.inner.chunk_mut()
    }

    #[inline]
    fn advance(&mut self, by: usize) -> Result<(), crate::io::Full> {
        self.inner.advance(by)
    }

    #[inline]
    fn remaining(&self) -> usize {
        self.inner.remaining()
    }

    #[inline]
    fn extend(&mut self, with: &[u8]) -> Result<(), crate::io::Full> {
        self.inner.extend(with)
    }
}

impl<'a, const N: usize> Writer for ArrayBufWriter<'a, N> {
    type Error = <PartiallyInitializedWriter<'a, [MaybeUninit<u8>; N]> as Writer>::Error;

    #[inline]
    fn write_buf<B: Buf>(&mut self, buf: B) -> Result<(), crate::io::Full> {
        self.inner.write_buf(buf)
    }

    #[inline]
    fn skip(&mut self, amount: usize) -> Result<(), crate::io::Full> {
        self.inner.skip(amount)
    }
}

#[cfg(test)]
mod tests {
    use super::ArrayBuf;
    use crate::{
        buf::{
            tests::buf_mut_tests,
            Full,
        },
        copy,
    };

    buf_mut_tests!(ArrayBuf::<20>::new());

    #[test]
    fn cant_write_more_than_buf_size() {
        let mut bytes_mut = ArrayBuf::<4>::new();
        assert_eq!(
            copy(&mut bytes_mut, b"abcdefgh").unwrap_err(),
            Full {
                required: 8,
                capacity: 4
            }
        );
    }
}
