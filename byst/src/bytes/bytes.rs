use std::{
    fmt::Debug,
    iter::FusedIterator,
};

use super::r#impl::{
    BytesImpl,
    ChunksIterImpl,
};
use crate::{
    buf::{
        Empty,
        Length,
    },
    util::{
        buf_eq,
        debug_as_hexdump,
    },
    Buf,
};

pub struct Bytes {
    inner: Box<dyn BytesImpl>,
}

impl Bytes {
    /// Creates an empty [`Bytes`].
    ///
    /// This doesn't allocate.
    #[inline]
    pub fn new() -> Self {
        // note: this really doesn't allocate, since [`Empty`] is a ZST, and a `dyn ZST`
        // is ZST itself.[1]
        //
        // [1]: https://users.rust-lang.org/t/what-does-box-dyn-actually-allocate/56618/2
        Self::from_impl(Box::new(Empty))
    }

    #[cfg(feature = "bytes-impl")]
    #[inline]
    pub fn from_impl(inner: Box<dyn BytesImpl>) -> Self {
        Self { inner }
    }

    #[cfg(not(feature = "bytes-impl"))]
    #[inline]
    pub(crate) fn from_impl(inner: Box<dyn BytesImpl>) -> Self {
        Self { inner }
    }
}

impl Default for Bytes {
    /// Creates an empty [`Bytes`].
    ///
    /// This doesn't allocate.
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Bytes {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl From<&'static [u8]> for Bytes {
    #[inline]
    fn from(value: &'static [u8]) -> Self {
        Self::from_impl(Box::new(value))
    }
}

impl Buf for Bytes {
    type View<'a> = Bytes
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
        Ok(Bytes::from_impl(self.inner.view(range.into())?))
    }

    #[inline]
    fn chunks(
        &self,
        range: impl Into<crate::Range>,
    ) -> Result<Self::Chunks<'_>, crate::RangeOutOfBounds> {
        Ok(Chunks::from_impl(Box::new(
            self.inner.chunks(range.into())?,
        )))
    }
}

impl Length for Bytes {
    #[inline]
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl Debug for Bytes {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        debug_as_hexdump(f, self)
    }
}

impl<T: Buf> PartialEq<T> for Bytes {
    #[inline]
    fn eq(&self, other: &T) -> bool {
        buf_eq(self, other)
    }
}

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
