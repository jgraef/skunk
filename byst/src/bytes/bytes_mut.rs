use std::fmt::Debug;

use super::{
    r#impl::BytesMutImpl,
    view::{
        View,
        ViewMut,
        ViewMutWriter,
    },
};
use crate::{
    buf::{
        arc_buf::ArcBufMut,
        Empty,
        Full,
        Length,
        SizeLimit,
    },
    util::{
        buf_eq,
        cfg_pub,
        debug_as_hexdump,
    },
    Buf,
    BufMut,
    Range,
    RangeOutOfBounds,
};

pub struct BytesMut {
    inner: ViewMut<'static>,
}

impl BytesMut {
    cfg_pub! {
        #[inline]
        pub(#[cfg(feature = "bytes-impl")]) fn from_impl(inner: Box<dyn BytesMutImpl>) -> Self {
            Self {
                inner: ViewMut::from_impl(inner),
            }
        }
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

impl<'b> Debug for BytesMut {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        debug_as_hexdump(f, self)
    }
}

impl<'b, R: Buf> PartialEq<R> for BytesMut {
    #[inline]
    fn eq(&self, other: &R) -> bool {
        buf_eq(self, other)
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

    type Reader<'a> = View<'a>
    where
        Self: 'a;

    #[inline]
    fn view(&self, range: impl Into<Range>) -> Result<Self::View<'_>, RangeOutOfBounds> {
        Ok(self.inner.view(range.into())?)
    }

    #[inline]
    fn reader(&self) -> Self::Reader<'_> {
        self.inner.reader()
    }
}

impl BufMut for BytesMut {
    type ViewMut<'a> = ViewMut<'a>
    where
        Self: 'a;

    type Writer<'a> = ViewMutWriter<'a>
    where
        Self: 'a;

    #[inline]
    fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds> {
        self.inner.view_mut(range)
    }

    #[inline]
    fn writer(&mut self) -> Self::Writer<'_> {
        self.inner.writer()
    }

    #[inline]
    fn reserve(&mut self, size: usize) -> Result<(), Full> {
        self.inner.reserve(size)
    }

    #[inline]
    fn size_limit(&self) -> SizeLimit {
        self.inner.size_limit()
    }
}
