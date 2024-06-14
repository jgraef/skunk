use super::Buf;
use crate::{
    dyn_impl::BytesImpl,
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
}

impl BytesImpl for Empty {
    #[inline]
    fn view(&self, _range: Range) -> Result<Box<dyn BytesImpl>, RangeOutOfBounds> {
        Ok(Box::new(Self::default()))
    }

    #[inline]
    fn chunks(
        &self,
        _range: Range,
    ) -> Result<Box<dyn Iterator<Item = &[u8]> + '_>, RangeOutOfBounds> {
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
