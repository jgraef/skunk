use std::iter::FusedIterator;

use super::r#impl::ChunksIterImpl;

pub struct Chunks<'a> {
    inner: Box<dyn ChunksIterImpl<'a> + 'a>,
}

impl<'a> Chunks<'a> {
    pub(crate) fn from_impl(inner: Box<dyn ChunksIterImpl<'a> + 'a>) -> Self {
        Self { inner }
    }
}

impl<'a> Iterator for Chunks<'a> {
    type Item = &'a [u8];

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<'a> DoubleEndedIterator for Chunks<'a> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back()
    }
}

impl<'a> FusedIterator for Chunks<'a> {}

impl<'a> ExactSizeIterator for Chunks<'a> {}
