use std::iter::FusedIterator;

use super::{
    Buf,
    BufReader,
    Length,
};

/// Iterator over the bytes in a buffer.
pub struct BufIter<'b, B: Buf + ?Sized + 'b> {
    reader: B::Reader<'b>,
}

impl<'b, B: Buf + ?Sized> BufIter<'b, B> {
    #[inline]
    pub fn new(buf: &'b B) -> Self {
        let reader = buf.reader();
        Self { reader }
    }
}

impl<'b, B: Buf + ?Sized> Iterator for BufIter<'b, B> {
    type Item = u8;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let chunk = self.reader.peek_chunk()?;
        let byte = *chunk.first().expect("BufReader returned empty chunk.;");
        self.reader
            .advance(1)
            .expect("BufReader failed to advance by 1");
        Some(byte)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.reader.remaining();
        (remaining, Some(remaining))
    }
}

impl<'b, B: Buf + ?Sized> FusedIterator for BufIter<'b, B> {}

impl<'b, B: Buf + ?Sized> ExactSizeIterator for BufIter<'b, B> {}

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
