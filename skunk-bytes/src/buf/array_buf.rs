use std::{
    fmt::Debug,
    mem::MaybeUninit,
    ops::{
        Deref,
        DerefMut,
    },
};

use super::partially_initialized::PartiallyInitialized;
use crate::{
    buf::{
        Buf,
        BufMut,
        SingleChunk,
        SingleChunkMut,
        SizeLimit,
        WriteError,
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
    fn grow_for(&mut self, range: impl Into<Range>) -> Result<(), RangeOutOfBounds> {
        self.inner.grow_for(range)
    }

    #[inline]
    fn write(
        &mut self,
        destination_range: impl Into<Range>,
        source: impl Buf,
        source_range: impl Into<Range>,
    ) -> Result<(), WriteError> {
        self.inner.write(destination_range, source, source_range)
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
        buf::WriteError,
        Buf,
        BufMut,
    };

    #[test]
    fn write_with_fill() {
        let mut bytes_mut = ArrayBuf::<128>::new();
        bytes_mut.write(4..8, b"abcd", ..).unwrap();
        assert_eq!(
            bytes_mut.chunks(..).unwrap().next().unwrap(),
            b"\x00\x00\x00\x00abcd"
        );
    }

    #[test]
    fn write_over_buf_end() {
        let mut bytes_mut = ArrayBuf::<128>::new();
        bytes_mut.write(0..4, b"abcd", ..).unwrap();
        bytes_mut.write(2..6, b"efgh", ..).unwrap();
        assert_eq!(bytes_mut.chunks(..).unwrap().next().unwrap(), b"abefgh");
    }

    #[test]
    fn write_extend_with_unbounded_destination_slice() {
        let mut bytes_mut = ArrayBuf::<128>::new();
        bytes_mut.write(0..4, b"abcd", ..).unwrap();
        bytes_mut.write(2.., b"efgh", ..).unwrap();
        assert_eq!(bytes_mut.chunks(..).unwrap().next().unwrap(), b"abefgh");
    }

    #[test]
    fn cant_write_more_than_buf_size() {
        let mut bytes_mut = ArrayBuf::<4>::new();
        assert_eq!(
            bytes_mut.write(0..8, b"abcdefgh", 0..8).unwrap_err(),
            WriteError::Full {
                required: (0..8).into(),
                buf_length: 4
            }
        );
    }
}
