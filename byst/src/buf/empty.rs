use super::Buf;
use crate::{
    bytes::r#impl::{
        BytesImpl,
        ChunksIterImpl,
    },
    Bytes,
    Range,
    RangeOutOfBounds,
};

/// An empty buffer.
#[derive(Debug, Clone, Copy, Default)]
pub struct Empty;

impl Buf for Empty {
    type View<'a> = Self
    where
        Self: 'a;

    type Chunks<'a> = std::iter::Empty<&'a [u8]>
    where
        Self: 'a;

    #[inline]
    fn view(&self, range: impl Into<Range>) -> Result<Self::View<'static>, RangeOutOfBounds> {
        range.into().indices_checked_in(0, 0)?;
        Ok(Self)
    }

    #[inline]
    fn chunks(&self, range: impl Into<Range>) -> Result<Self::Chunks<'static>, RangeOutOfBounds> {
        range.into().indices_checked_in(0, 0)?;
        Ok(std::iter::empty())
    }

    #[inline]
    fn len(&self) -> usize {
        0
    }

    #[inline]
    fn is_empty(&self) -> bool {
        true
    }
}

impl BytesImpl for Empty {
    #[inline]
    fn view(&self, _range: Range) -> Result<Box<dyn BytesImpl>, RangeOutOfBounds> {
        Ok(Box::new(Self::default()))
    }

    #[inline]
    fn chunks(&self, _range: Range) -> Result<Box<dyn ChunksIterImpl<'_> + '_>, RangeOutOfBounds> {
        Ok(Box::new(std::iter::empty()))
    }

    #[inline]
    fn len(&self) -> usize {
        0
    }

    #[inline]
    fn clone(&self) -> Box<dyn BytesImpl> {
        Box::new(Self::default())
    }
}

impl From<Empty> for Bytes {
    #[inline]
    fn from(value: Empty) -> Self {
        Self::from_impl(Box::new(value))
    }
}
