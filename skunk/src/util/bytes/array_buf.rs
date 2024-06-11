use std::{
    fmt::Debug,
    mem::MaybeUninit,
    ops::{
        Deref,
        DerefMut,
    },
};

use super::{
    buf::{
        write_helper,
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
    // invariant: `buf[..initialized]` is initialized.
    buf: [MaybeUninit<u8>; N],
    initialized: usize,
}

impl<const N: usize> ArrayBuf<N> {
    #[inline]
    pub fn new() -> Self {
        Self {
            buf: [MaybeUninit::uninit(); N],
            initialized: 0,
        }
    }

    #[inline]
    fn bytes(&self) -> &[u8] {
        // invariant: this will always return a slice of length `self.initialized`.

        // SAFETY: see invariant in struct
        unsafe { MaybeUninit::slice_assume_init_ref(&self.buf[..self.initialized]) }
    }

    #[inline]
    fn bytes_mut(&mut self) -> &mut [u8] {
        // invariant: this will always return a slice of length `self.initialized`.

        // SAFETY: see invariant in struct
        unsafe { MaybeUninit::slice_assume_init_mut(&mut self.buf[..self.initialized]) }
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.initialized == N
    }

    pub fn resize(&mut self, new_len: usize, value: u8) {
        if new_len > N {
            panic!("Can't resize ArrayBuf<{N}> to length {new_len}");
        }

        // after this the struct invariant still holds
        if new_len > self.initialized {
            MaybeUninit::fill(&mut self.buf[self.initialized..new_len], value);
        }
        self.initialized = new_len;
    }

    #[inline]
    pub fn clear(&mut self) {
        self.initialized = 0;
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
        self.bytes()
    }
}

impl<const N: usize> AsMut<[u8]> for ArrayBuf<N> {
    #[inline]
    fn as_mut(&mut self) -> &mut [u8] {
        self.bytes_mut()
    }
}

impl<const N: usize> Deref for ArrayBuf<N> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        self.bytes()
    }
}

impl<const N: usize> DerefMut for ArrayBuf<N> {
    #[inline]
    fn deref_mut(&mut self) -> &mut [u8] {
        self.bytes_mut()
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
        MaybeUninit::copy_from_slice(&mut cloned.buf, self.bytes());
        cloned.initialized = self.initialized;

        cloned
    }
}

impl<const N: usize> Copy for ArrayBuf<N> {}

impl<const N: usize> Debug for ArrayBuf<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.bytes()).finish()
    }
}

impl<const N: usize> PartialEq for ArrayBuf<N> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.bytes() == other.bytes()
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
        Ok(range.into().slice_get(self.bytes())?)
    }

    #[inline]
    fn chunks(&self, range: impl Into<Range>) -> Result<Self::Chunks<'_>, RangeOutOfBounds> {
        Ok(SingleChunk::new(range.into().slice_get(self.bytes())?))
    }

    #[inline]
    fn len(&self) -> usize {
        self.initialized
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
        Ok(range.into().slice_get_mut(self.bytes_mut())?)
    }

    #[inline]
    fn chunks_mut(
        &mut self,
        range: impl Into<Range>,
    ) -> Result<Self::ChunksMut<'_>, RangeOutOfBounds> {
        Ok(SingleChunkMut::new(
            range.into().slice_get_mut(self.bytes_mut())?,
        ))
    }

    fn grow_for(&mut self, range: impl Into<Range>) -> Result<(), RangeOutOfBounds> {
        let range = range.into();
        let new_len = range.len_in(0, self.len());
        if new_len <= N {
            self.resize(new_len, 0);
            Ok(())
        }
        else {
            Err(RangeOutOfBounds {
                required: range,
                bounds: (0, N),
            })
        }
    }

    fn write(
        &mut self,
        destination_range: impl Into<Range>,
        source: impl Buf,
        source_range: impl Into<Range>,
    ) -> Result<(), WriteError> {
        write_helper(
            self,
            destination_range,
            &source,
            source_range,
            |_this, n| (n <= N).then_some(()).ok_or(N),
            |_, _| (),
            |this, n| this.resize(n, 0),
            |this, chunk| {
                MaybeUninit::copy_from_slice(
                    &mut this.buf[this.initialized..][..chunk.len()],
                    chunk,
                );
                this.initialized += chunk.len();
            },
        )
    }

    #[inline]
    fn size_limit(&self) -> SizeLimit {
        N.into()
    }
}
