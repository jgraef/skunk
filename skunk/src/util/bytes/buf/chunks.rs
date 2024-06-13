use std::iter::{
    Flatten,
    FusedIterator,
};

use super::{
    Buf,
    BufMut,
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
    inner: Flatten<B::Chunks<'b>>,
}

impl<'b, B: Buf + ?Sized> BufIter<'b, B> {
    #[inline]
    pub fn new(chunks: B::Chunks<'b>) -> Self {
        Self {
            inner: chunks.flatten(),
        }
    }
}

impl<'b, B: Buf + ?Sized> Iterator for BufIter<'b, B> {
    type Item = u8;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().copied()
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

/// Mutable iterator over the bytes in a buffer.
pub struct BufIterMut<'b, B: BufMut + ?Sized + 'b> {
    inner: Flatten<B::ChunksMut<'b>>,
}

impl<'b, B: BufMut + ?Sized> BufIterMut<'b, B> {
    #[inline]
    pub fn new(chunks: B::ChunksMut<'b>) -> Self {
        Self {
            inner: chunks.flatten(),
        }
    }
}

impl<'b, B: BufMut + ?Sized> Iterator for BufIterMut<'b, B> {
    type Item = &'b mut u8;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
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

/// Iterator wrapper to skip empty chunks.
#[derive(Debug)]
pub struct NonEmptyIter<I>(pub I);

impl<T: AsRef<[u8]>, I: Iterator<Item = T>> Iterator for NonEmptyIter<I> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let x = self.0.next()?;
        (!x.as_ref().is_empty()).then_some(x)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, self.0.size_hint().1)
    }
}

impl<T: AsRef<[u8]>, I: Iterator<Item = T> + DoubleEndedIterator> DoubleEndedIterator
    for NonEmptyIter<I>
{
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back()
    }
}

impl<T: AsRef<[u8]>, I: Iterator<Item = T>> FusedIterator for NonEmptyIter<I> {}
