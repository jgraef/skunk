use std::iter::{
    Flatten,
    Fuse,
    FusedIterator,
};

use super::{
    Buf,
    BufMut,
    Length,
};

/// Chunk iterator for contiguous buffers.
#[derive(Clone, Copy, Debug)]
pub struct SingleChunk<'a> {
    chunk: &'a [u8],
    exhausted: bool,
}

impl<'a> SingleChunk<'a> {
    #[inline]
    pub fn new(chunk: &'a [u8]) -> Self {
        Self {
            chunk,
            exhausted: chunk.is_empty(),
        }
    }

    #[inline]
    pub fn get(&self) -> &'a [u8] {
        self.chunk
    }
}

impl<'a> Iterator for SingleChunk<'a> {
    type Item = &'a [u8];

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.exhausted {
            None
        }
        else {
            self.exhausted = true;
            Some(self.chunk)
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.exhausted.then_some(0).unwrap_or(1);
        (n, Some(n))
    }
}

impl<'a> DoubleEndedIterator for SingleChunk<'a> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.next()
    }
}

impl<'a> ExactSizeIterator for SingleChunk<'a> {}

impl<'a> FusedIterator for SingleChunk<'a> {}

/// Mutable chunk iterator for contiguous buffers.
#[derive(Debug)]
pub struct SingleChunkMut<'a> {
    chunk: Option<&'a mut [u8]>,
}

impl<'a> SingleChunkMut<'a> {
    #[inline]
    pub fn new(chunk: &'a mut [u8]) -> Self {
        Self {
            chunk: (!chunk.is_empty()).then_some(chunk),
        }
    }
}

impl<'a> Iterator for SingleChunkMut<'a> {
    type Item = &'a mut [u8];

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.chunk.take()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.chunk.is_some().then_some(1).unwrap_or_default();
        (n, Some(n))
    }
}

impl<'a> DoubleEndedIterator for SingleChunkMut<'a> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.chunk.take()
    }
}

impl<'a> ExactSizeIterator for SingleChunkMut<'a> {}

impl<'a> FusedIterator for SingleChunkMut<'a> {}

/// Iterator over the bytes in a buffer.
#[derive(Debug)]
pub struct BufIter<'b, B: Buf + ?Sized + 'b> {
    inner: Flatten<Fuse<B::Chunks<'b>>>,
    len: usize,
}

impl<'b, B: Buf + ?Sized> BufIter<'b, B> {
    #[inline]
    pub(crate) fn new(chunks: B::Chunks<'b>, len: usize) -> Self {
        Self {
            inner: chunks.fuse().flatten(),
            len,
        }
    }
}

impl<'b, B: Buf + ?Sized> Iterator for BufIter<'b, B> {
    type Item = u8;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().copied()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<'b, B: Buf + ?Sized> DoubleEndedIterator for BufIter<'b, B>
where
    <B as Buf>::Chunks<'b>: DoubleEndedIterator,
{
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back().copied()
    }
}

impl<'b, B: Buf + ?Sized> FusedIterator for BufIter<'b, B> {}

impl<'b, B: Buf + ?Sized> ExactSizeIterator for BufIter<'b, B> {}

/// Mutable iterator over the bytes in a buffer.
pub struct BufIterMut<'b, B: BufMut + ?Sized + 'b> {
    inner: Flatten<Fuse<B::ChunksMut<'b>>>,
    len: usize,
}

impl<'b, B: BufMut + ?Sized> BufIterMut<'b, B> {
    #[inline]
    pub(crate) fn new(chunks: B::ChunksMut<'b>, len: usize) -> Self {
        Self {
            inner: chunks.fuse().flatten(),
            len,
        }
    }
}

impl<'b, B: BufMut + ?Sized> Iterator for BufIterMut<'b, B> {
    type Item = &'b mut u8;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl<'b, B: BufMut + ?Sized> DoubleEndedIterator for BufIterMut<'b, B>
where
    <B as BufMut>::ChunksMut<'b>: DoubleEndedIterator,
{
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back()
    }
}

impl<'b, B: BufMut + ?Sized> FusedIterator for BufIterMut<'b, B> {}

impl<'b, B: BufMut + ?Sized> ExactSizeIterator for BufIterMut<'b, B> {}

/// Iterator wrapper to skip empty chunks.
#[derive(Debug)]
pub struct NonEmpty<I> {
    inner: I,
}

impl<I> NonEmpty<I> {
    #[inline]
    pub fn new(inner: I) -> Self {
        Self { inner }
    }

    #[inline]
    pub fn into_inner(self) -> I {
        self.inner
    }
}

impl<T: Length, I: Iterator<Item = T>> Iterator for NonEmpty<I> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let item = self.inner.next()?;
        (!item.is_empty()).then_some(item)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, self.inner.size_hint().1)
    }
}

impl<T: Length, I: Iterator<Item = T> + DoubleEndedIterator> DoubleEndedIterator for NonEmpty<I> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back()
    }
}

impl<T: Length, I: Iterator<Item = T> + FusedIterator> FusedIterator for NonEmpty<I> {}

/// Wrapper for chunk iterators that also tracks the current offset.
pub struct WithOffset<I> {
    inner: I,
    offset: usize,
}

impl<I> WithOffset<I> {
    #[inline]
    pub fn new(inner: I) -> Self {
        Self::with_offset(inner, 0)
    }

    #[inline]
    pub fn with_offset(inner: I, offset: usize) -> Self {
        Self { inner, offset }
    }

    #[inline]
    pub fn offset(&self) -> usize {
        self.offset
    }

    #[inline]
    pub fn into_inner(self) -> I {
        self.inner
    }
}

impl<T: Length, I: Iterator<Item = T>> Iterator for WithOffset<I> {
    type Item = (usize, T);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let chunk = self.inner.next()?;
        let offset = self.offset;
        self.offset += chunk.len();
        Some((offset, chunk))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<T: Length, I: Iterator<Item = T> + FusedIterator> FusedIterator for WithOffset<I> {}

pub trait ChunksExt: Sized {
    fn non_empty(self) -> NonEmpty<Self>;
    fn with_offset(self) -> WithOffset<Self>;
}

impl<'a, I: Iterator<Item = &'a [u8]>> ChunksExt for I {
    #[inline]
    fn non_empty(self) -> NonEmpty<Self> {
        NonEmpty::new(self)
    }

    #[inline]
    fn with_offset(self) -> WithOffset<Self> {
        WithOffset::new(self)
    }
}

pub trait ChunksMutExt: Sized {
    fn non_empty(self) -> NonEmpty<Self>;
    fn with_offset(self) -> WithOffset<Self>;
}

impl<'a, I: Iterator<Item = &'a mut [u8]>> ChunksMutExt for I {
    #[inline]
    fn non_empty(self) -> NonEmpty<Self> {
        NonEmpty::new(self)
    }

    #[inline]
    fn with_offset(self) -> WithOffset<Self> {
        WithOffset::new(self)
    }
}
