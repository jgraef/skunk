use std::fmt::{
    Debug,
    Display,
};

use super::{
    chunks::Chunks,
    r#impl::BytesImpl,
};
use crate::{
    buf::Empty,
    util::Peekable,
    Buf,
};

pub struct Bytes {
    inner: Box<dyn BytesImpl>,
}

impl Bytes {
    #[inline]
    pub(crate) fn from_impl(inner: Box<dyn BytesImpl>) -> Self {
        Self { inner }
    }
}

impl Default for Bytes {
    /// Creates an empty [`Bytes`].
    #[inline]
    fn default() -> Self {
        Self::from(Empty)
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
        use crate::hexdump::{
            Config,
            Hexdump,
        };
        let hex = Hexdump::with_config(
            self,
            Config {
                offset: 0,
                trailing_newline: false,
                at_least_one_line: false,
                header: false,
            },
        );
        Display::fmt(&hex, f)
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
