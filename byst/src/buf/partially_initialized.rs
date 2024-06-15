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

use super::Full;
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

    fn reserve(&mut self, size: usize) -> Result<(), Full> {
        let n = self.buf.as_ref().len();
        if size <= n {
            Ok(())
        }
        else {
            Err(Full {
                required: size,
                buf_length: n,
            })
        }
    }

    fn grow(&mut self, new_len: usize, value: u8) -> Result<(), Full> {
        let n = self.buf.as_ref().len();
        if new_len <= n {
            // after this the struct invariant still holds
            if new_len > self.initialized {
                MaybeUninit::fill(&mut self.buf.as_mut()[self.initialized..new_len], value);
            }
            self.initialized = new_len;
            Ok(())
        }
        else {
            Err(Full {
                required: new_len,
                buf_length: n,
            })
        }
    }

    fn extend(&mut self, with: &[u8]) -> Result<(), Full> {
        let n = self.buf.as_ref().len();
        let new_len = self.initialized + with.len();
        if new_len <= n {
            // after this the struct invariant still holds
            MaybeUninit::copy_from_slice(&mut self.buf.as_mut()[self.initialized..new_len], with);
            self.initialized = new_len;
            Ok(())
        }
        else {
            Err(Full {
                required: new_len,
                buf_length: n,
            })
        }
    }

    #[inline]
    fn size_limit(&self) -> SizeLimit {
        self.buf.as_ref().len().into()
    }
}
