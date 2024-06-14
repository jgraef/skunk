//! # Note
//!
//! This is intentionally not public. If used with a type `B`, which doesn't
//! uphold some invariants, this is unsafe.

use std::{
    fmt::Debug,
    mem::MaybeUninit,
    ops::{
        Deref,
        DerefMut,
    },
};

use crate::{
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

/// A contiguous chunk of memory that is partially initialized.
#[derive(Copy, Clone)]
pub struct PartiallyInitialized<B> {
    // invariant: `buf[..initialized]` is initialized.
    buf: B,
    initialized: usize,
}

impl<B> PartiallyInitialized<B> {
    #[inline]
    pub fn new(buf: B) -> Self {
        Self {
            buf,
            initialized: 0,
        }
    }

    #[inline]
    pub unsafe fn assume_initialized(&mut self, initialized: usize) {
        self.initialized = initialized;
    }

    #[inline]
    pub fn clear(&mut self) {
        self.initialized = 0;
    }

    #[inline]
    pub fn inner_ref(&self) -> &B {
        &self.buf
    }

    #[inline]
    pub fn inner_mut(&mut self) -> &mut B {
        &mut self.buf
    }
}

impl<B: AsRef<[MaybeUninit<u8>]>> PartiallyInitialized<B> {
    #[inline]
    fn bytes(&self) -> &[u8] {
        // invariant: this will always return a slice of length `self.initialized`.

        // SAFETY: see invariant in struct
        unsafe { MaybeUninit::slice_assume_init_ref(&self.buf.as_ref()[..self.initialized]) }
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.initialized == self.buf.as_ref().len()
    }
}

impl<B: AsMut<[MaybeUninit<u8>]>> PartiallyInitialized<B> {
    #[inline]
    fn bytes_mut(&mut self) -> &mut [u8] {
        // invariant: this will always return a slice of length `self.initialized`.

        // SAFETY: see invariant in struct
        unsafe { MaybeUninit::slice_assume_init_mut(&mut self.buf.as_mut()[..self.initialized]) }
    }

    pub fn resize(&mut self, new_len: usize, value: u8) {
        let n = self.buf.as_mut().len();
        if new_len > n {
            panic!("Can't resize ArrayBuf<{n}> to length {new_len}");
        }

        // after this the struct invariant still holds
        if new_len > self.initialized {
            MaybeUninit::fill(&mut self.buf.as_mut()[self.initialized..new_len], value);
        }
        self.initialized = new_len;
    }
}

impl<B: AsRef<[MaybeUninit<u8>]>> AsRef<[u8]> for PartiallyInitialized<B> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.bytes()
    }
}

impl<B: AsMut<[MaybeUninit<u8>]>> AsMut<[u8]> for PartiallyInitialized<B> {
    #[inline]
    fn as_mut(&mut self) -> &mut [u8] {
        self.bytes_mut()
    }
}

impl<B: AsRef<[MaybeUninit<u8>]>> Deref for PartiallyInitialized<B> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        self.bytes()
    }
}

impl<B: AsRef<[MaybeUninit<u8>]> + AsMut<[MaybeUninit<u8>]>> DerefMut for PartiallyInitialized<B> {
    #[inline]
    fn deref_mut(&mut self) -> &mut [u8] {
        self.bytes_mut()
    }
}

impl<B: AsRef<[MaybeUninit<u8>]>> Debug for PartiallyInitialized<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.bytes()).finish()
    }
}

impl<B: AsRef<[MaybeUninit<u8>]>> PartialEq for PartiallyInitialized<B> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.bytes() == other.bytes()
    }
}

impl<B: AsRef<[MaybeUninit<u8>]>> Eq for PartiallyInitialized<B> {}

impl<B: AsRef<[MaybeUninit<u8>]>> Buf for PartiallyInitialized<B> {
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

impl<B: AsRef<[MaybeUninit<u8>]> + AsMut<[MaybeUninit<u8>]>> BufMut for PartiallyInitialized<B> {
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
        let n = self.buf.as_ref().len();
        if new_len <= n {
            self.resize(new_len, 0);
            Ok(())
        }
        else {
            Err(RangeOutOfBounds {
                required: range,
                bounds: (0, n),
            })
        }
    }

    fn write(
        &mut self,
        destination_range: impl Into<Range>,
        source: impl Buf,
        source_range: impl Into<Range>,
    ) -> Result<(), WriteError> {
        let len = self.buf.as_ref().len();
        write_helper(
            self,
            destination_range,
            &source,
            source_range,
            |_this, n| (n <= len).then_some(()).ok_or(len),
            |_, _| (),
            |this, n| this.resize(n, 0),
            |this, chunk| {
                MaybeUninit::copy_from_slice(
                    &mut this.buf.as_mut()[this.initialized..][..chunk.len()],
                    chunk,
                );
                this.initialized += chunk.len();
            },
        )
    }

    #[inline]
    fn size_limit(&self) -> SizeLimit {
        self.buf.as_ref().len().into()
    }
}
