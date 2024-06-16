#![allow(dead_code)]

use crate::{
    buf::chunks::SingleChunk,
    Range,
    RangeOutOfBounds,
};

/// The trait backing the [`Bytes`] implementation.
///
/// Implement this for your type, for it to be usable as a [`Bytes`]. Use
/// [`Bytes::from_impl`] to implement a conversion from your type to [`Bytes`].
pub trait BytesImpl {
    fn view(&self, range: Range) -> Result<Box<dyn BytesImpl>, RangeOutOfBounds>;
    fn chunks<'a>(
        &'a self,
        range: Range,
    ) -> Result<Box<dyn ChunksIterImpl<'a> + 'a>, RangeOutOfBounds>;
    fn len(&self) -> usize;
    fn clone(&self) -> Box<dyn BytesImpl>;
}

impl BytesImpl for &'static [u8] {
    #[inline]
    fn view(&self, range: Range) -> Result<Box<dyn BytesImpl>, RangeOutOfBounds> {
        Ok(Box::new(range.slice_get(*self)?))
    }

    fn chunks(&self, range: Range) -> Result<Box<dyn ChunksIterImpl<'_> + '_>, RangeOutOfBounds> {
        Ok(Box::new(SingleChunk::new(range.slice_get(*self)?)))
    }

    fn len(&self) -> usize {
        <[u8]>::len(*self)
    }

    fn clone(&self) -> Box<dyn BytesImpl> {
        Box::new(*self)
    }
}

pub trait ChunksIterImpl<'a>: Iterator<Item = &'a [u8]> + DoubleEndedIterator {}

impl<'a, T: Iterator<Item = &'a [u8]> + DoubleEndedIterator> ChunksIterImpl<'a> for T {}

pub trait BytesMutImpl: BytesImpl {
    fn chunks_mut(
        &mut self,
        range: Range,
    ) -> Result<Box<dyn Iterator<Item = &mut [u8]> + '_>, RangeOutOfBounds>;
}
