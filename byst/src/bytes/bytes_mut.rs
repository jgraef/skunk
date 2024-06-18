use std::{
    iter::FusedIterator,
    sync::Arc,
};

use super::{
    bytes::Chunks,
    r#impl::{
        BytesMutImpl,
        BytesMutViewImpl,
        BytesMutViewMutImpl,
        ChunksMutIterImpl,
    },
};
use crate::{
    buf::{
        arc_buf::ArcBufMut,
        rope::Segment,
        Empty,
        Full,
        Length,
        SizeLimit,
    },
    Buf,
    BufMut,
    Range,
    RangeOutOfBounds,
};

pub struct BytesMut {
    inner: Box<dyn BytesMutImpl>,
}

impl BytesMut {
    #[cfg(feature = "bytes-impl")]
    #[inline]
    pub fn from_impl(inner: Box<dyn BytesMutImpl>) -> Self {
        Self { inner }
    }

    #[cfg(not(feature = "bytes-impl"))]
    #[inline]
    pub(crate) fn from_impl(inner: Box<dyn BytesMutImpl>) -> Self {
        Self { inner }
    }

    #[inline]
    pub fn new() -> Self {
        Self::from_impl(Box::new(Empty))
    }

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self::from_impl(Box::new(ArcBufMut::new(capacity)))
    }
}

impl Default for BytesMut {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Length for BytesMut {
    #[inline]
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl Buf for BytesMut {
    type View<'a> = View<'a>
    where
        Self: 'a;

    type Chunks<'a> = Chunks<'a>
    where
        Self: 'a;

    #[inline]
    fn view(
        &self,
        range: impl Into<crate::Range>,
    ) -> Result<Self::View<'_>, crate::RangeOutOfBounds> {
        Ok(View::from_impl(self.inner.view(range.into())?))
    }

    #[inline]
    fn chunks(
        &self,
        range: impl Into<crate::Range>,
    ) -> Result<Self::Chunks<'_>, crate::RangeOutOfBounds> {
        Ok(Chunks::from_impl(self.inner.chunks(range.into())?))
    }
}

impl BufMut for BytesMut {
    type ViewMut<'a> = ViewMut<'a>
    where
        Self: 'a;

    type ChunksMut<'a> = ChunksMut<'a>
    where
        Self: 'a;

    #[inline]
    fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds> {
        Ok(ViewMut::from_impl(self.inner.view_mut(range.into())?))
    }

    #[inline]
    fn chunks_mut(
        &mut self,
        range: impl Into<Range>,
    ) -> Result<Self::ChunksMut<'_>, RangeOutOfBounds> {
        Ok(ChunksMut::from_impl(self.inner.chunks_mut(range.into())?))
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
        self.inner.size_limit()
    }
}

pub struct View<'b> {
    inner: Box<dyn BytesMutViewImpl<'b> + 'b>,
}

impl<'b> View<'b> {
    pub(crate) fn from_impl(inner: Box<dyn BytesMutViewImpl<'b> + 'b>) -> Self {
        Self { inner }
    }
}

impl<'b> Length for View<'b> {
    #[inline]
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<'b> Buf for View<'b> {
    type View<'a> = View<'b>
    where
        Self: 'a;

    type Chunks<'a> = Chunks<'a>
    where
        Self: 'a;

    #[inline]
    fn view(&self, range: impl Into<Range>) -> Result<Self::View<'b>, RangeOutOfBounds> {
        Ok(View::from_impl(self.inner.view(range.into())?))
    }

    #[inline]
    fn chunks(&self, range: impl Into<Range>) -> Result<Self::Chunks<'_>, RangeOutOfBounds> {
        Ok(Chunks::from_impl(self.inner.chunks(range.into())?))
    }
}

pub struct ViewMut<'b> {
    inner: Box<dyn BytesMutViewMutImpl<'b> + 'b>,
}

impl<'b> ViewMut<'b> {
    pub(crate) fn from_impl(inner: Box<dyn BytesMutViewMutImpl<'b> + 'b>) -> Self {
        Self { inner }
    }
}

impl<'b> Length for ViewMut<'b> {
    #[inline]
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<'b> Buf for ViewMut<'b> {
    type View<'a> = View<'a>
    where
        Self: 'a;

    type Chunks<'a> = Chunks<'a>
    where
        Self: 'a;

    #[inline]
    fn view(&self, range: impl Into<Range>) -> Result<Self::View<'_>, RangeOutOfBounds> {
        Ok(View::from_impl(self.inner.view(range.into())?))
    }

    #[inline]
    fn chunks(&self, range: impl Into<Range>) -> Result<Self::Chunks<'_>, RangeOutOfBounds> {
        Ok(Chunks::from_impl(self.inner.chunks(range.into())?))
    }
}

impl<'b> BufMut for ViewMut<'b> {
    type ViewMut<'a> = ViewMut<'a>
    where
        Self: 'a;

    type ChunksMut<'a> = ChunksMut<'a>
    where
        Self: 'a;

    #[inline]
    fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds> {
        Ok(ViewMut::from_impl(self.inner.view_mut(range.into())?))
    }

    #[inline]
    fn chunks_mut(
        &mut self,
        range: impl Into<Range>,
    ) -> Result<Self::ChunksMut<'_>, RangeOutOfBounds> {
        Ok(ChunksMut::from_impl(self.inner.chunks_mut(range.into())?))
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
        self.inner.size_limit()
    }
}

pub struct ChunksMut<'a> {
    inner: Box<dyn ChunksMutIterImpl<'a> + 'a>,
}

impl<'a> ChunksMut<'a> {
    pub(crate) fn from_impl(inner: Box<dyn ChunksMutIterImpl<'a> + 'a>) -> Self {
        Self { inner }
    }
}

impl<'a> Iterator for ChunksMut<'a> {
    type Item = &'a mut [u8];

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<'a> DoubleEndedIterator for ChunksMut<'a> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back()
    }
}

impl<'a> FusedIterator for ChunksMut<'a> {}

impl<'a> ExactSizeIterator for ChunksMut<'a> {}

struct SpillOver {
    segments: Arc<Vec<Segment<Box<dyn BytesMutImpl>>>>,
}

impl SpillOver {
    pub fn new(inner: Box<dyn BytesMutImpl>) -> Self {
        Self {
            segments: Arc::new(vec![Segment {
                offset: 0,
                buf: inner,
            }]),
        }
    }
}
