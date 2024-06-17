use std::fmt::Debug;

use super::{
    chunks::Chunks,
    r#impl::BytesImpl,
};
use crate::{
    buf::Empty,
    util::{
        debug_as_hexdump,
        Peekable,
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

    #[inline]
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl Debug for Bytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        debug_as_hexdump(f, self)
    }
}

impl PartialEq for Bytes {
    fn eq(&self, other: &Self) -> bool {
        if self.len() != other.len() {
            return false;
        }

        let mut left_offset = 0;
        let mut right_offset = 0;

        let mut left_chunks = Peekable::new(self.chunks(..).unwrap());
        let mut right_chunks = Peekable::new(other.chunks(..).unwrap());

        loop {
            match (left_chunks.peek(), right_chunks.peek()) {
                (None, None) => unreachable!("Expected both Bytes to be of different lengths."),
                (Some(_), None) | (None, Some(_)) => break false,
                (Some(left), Some(right)) => {
                    let n = std::cmp::min(
                        <[u8]>::len(left) - left_offset,
                        <[u8]>::len(right) - right_offset,
                    );

                    if left[left_offset..][..n] != right[right_offset..][..n] {
                        break false;
                    }

                    left_offset += n;
                    right_offset += n;

                    if left_offset == <[u8]>::len(left) {
                        left_offset = 0;
                        left_chunks.next();
                    }
                    if right_offset == <[u8]>::len(right) {
                        right_offset = 0;
                        right_chunks.next();
                    }
                }
            }
        }
    }
}
