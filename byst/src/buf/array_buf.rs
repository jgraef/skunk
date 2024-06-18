use std::{
    fmt::Debug,
    mem::MaybeUninit,
    ops::{
        Deref,
        DerefMut,
    },
};

use super::{
    partially_initialized::PartiallyInitialized,
    Full,
    Length,
};
use crate::{
    buf::{
        Buf,
        BufMut,
        SingleChunk,
        SingleChunkMut,
        SizeLimit,
    },
    range::{
        Range,
        RangeOutOfBounds,
    },
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

impl<const N: usize> Deref for ArrayBuf<N> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        self.inner.deref()
    }
}

impl<const N: usize> DerefMut for ArrayBuf<N> {
    #[inline]
    fn deref_mut(&mut self) -> &mut [u8] {
        self.inner.deref_mut()
    }
}

impl<const N: usize> Clone for ArrayBuf<N> {
    #[inline]
    fn clone(&self) -> Self {
        // note: we could auto-derive this, since `MaybeUninit` implements `Clone` if
        // `T` (i.e. `u8`) is `Copy`. But it would copy the whole buffer, and we only
        // copy the portion that has been initialized.

        // note: since we have the invariant that `Self::bytes` always returns
        // `initialized` number of bytes, the `cloned` `ArrayBuf` will be initialized
        // correctly.

        let mut cloned = Self::new();
        let source = self.inner.deref();

        MaybeUninit::copy_from_slice(cloned.inner.inner_mut(), source);
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

impl<const N: usize> PartialEq for ArrayBuf<N> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        &self.inner == &other.inner
    }
}

impl<const N: usize> Eq for ArrayBuf<N> {}

impl<const N: usize> Buf for ArrayBuf<N> {
    type View<'a> = &'a [u8]
    where
        Self: 'a;

    type Chunks<'a> = SingleChunk<'a>
    where
        Self: 'a;

    #[inline]
    fn view(&self, range: impl Into<Range>) -> Result<Self::View<'_>, RangeOutOfBounds> {
        self.inner.view(range)
    }

    #[inline]
    fn chunks(&self, range: impl Into<Range>) -> Result<Self::Chunks<'_>, RangeOutOfBounds> {
        self.inner.chunks(range)
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

    type ChunksMut<'a> = SingleChunkMut<'a>
    where
        Self: 'a;

    #[inline]
    fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds> {
        self.inner.view_mut(range)
    }

    #[inline]
    fn chunks_mut(
        &mut self,
        range: impl Into<Range>,
    ) -> Result<Self::ChunksMut<'_>, RangeOutOfBounds> {
        self.inner.chunks_mut(range)
    }

    #[inline]
    fn reserve(&mut self, size: usize) -> Result<(), Full> {
        self.inner.reserve(size)
    }

    #[inline]
    fn grow(&mut self, new_len: usize, value: u8) -> Result<(), Full> {
        self.inner.grow(new_len, value)
    }

    #[inline]
    fn extend(&mut self, with: &[u8]) -> Result<(), Full> {
        self.inner.extend(with)
    }

    #[inline]
    fn size_limit(&self) -> SizeLimit {
        N.into()
    }
}

#[cfg(test)]
mod tests {
    use super::ArrayBuf;
    use crate::{
        buf::{
            copy::{
                copy,
                CopyError,
            },
            Full,
        },
        Buf,
    };

    #[test]
    fn write_with_fill() {
        let mut bytes_mut = ArrayBuf::<128>::new();
        copy(&mut bytes_mut, 4..8, b"abcd", ..).unwrap();
        assert_eq!(
            bytes_mut.chunks(..).unwrap().next().unwrap(),
            b"\x00\x00\x00\x00abcd"
        );
    }

    #[test]
    fn write_over_buf_end() {
        let mut bytes_mut = ArrayBuf::<128>::new();
        copy(&mut bytes_mut, 0..4, b"abcd", ..).unwrap();
        copy(&mut bytes_mut, 2..6, b"efgh", ..).unwrap();
        assert_eq!(bytes_mut.chunks(..).unwrap().next().unwrap(), b"abefgh");
    }

    #[test]
    fn write_extend_with_unbounded_destination_slice() {
        let mut bytes_mut = ArrayBuf::<128>::new();
        copy(&mut bytes_mut, 0..4, b"abcd", ..).unwrap();
        copy(&mut bytes_mut, 2.., b"efgh", ..).unwrap();
        assert_eq!(bytes_mut.chunks(..).unwrap().next().unwrap(), b"abefgh");
    }

    #[test]
    fn cant_write_more_than_buf_size() {
        let mut bytes_mut = ArrayBuf::<4>::new();
        assert_eq!(
            copy(&mut bytes_mut, 0..8, b"abcdefgh", 0..8).unwrap_err(),
            CopyError::Full(Full {
                required: 8,
                capacity: 4
            })
        );
    }
}
