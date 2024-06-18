#![allow(dead_code)]

use crate::{
    buf::{
        Full,
        Length,
        SizeLimit,
    },
    Buf,
    BufMut,
    IndexOutOfBounds,
    Range,
    RangeOutOfBounds,
};

/// The trait backing the [`Bytes`] implementation.
///
/// Implement this for your type, for it to be usable as a [`Bytes`]. Use
/// [`Bytes::from_impl`] to implement a conversion from your type to [`Bytes`].
///
/// [`Bytes`]: super::Bytes
/// [`Bytes::from_impl`]: super::Bytes::from_impl
pub trait BytesImpl: Length {
    fn view(&self, range: Range) -> Result<Box<dyn BytesImpl>, RangeOutOfBounds>;
    fn chunks<'a>(
        &'a self,
        range: Range,
    ) -> Result<Box<dyn ChunksIterImpl<'a> + 'a>, RangeOutOfBounds>;
    fn clone(&self) -> Box<dyn BytesImpl>;
}

/// The trait backing the [`BytesMut`] implementation.
///
/// Implement this for your type, for it to be usable as a [`BytesMut`]. Use
/// [`BytesMut::from_impl`] to implement a conversion from your type to
/// [`Bytes`].
///
/// [`BytesMut`]: super::BytesMut
/// [`BytesMut::from_impl`]: super::BytesMut::from_impl
pub trait BytesMutImpl: Length {
    fn view(&self, range: Range) -> Result<Box<dyn BytesMutViewImpl + '_>, RangeOutOfBounds>;
    fn view_mut(
        &mut self,
        range: Range,
    ) -> Result<Box<dyn BytesMutViewMutImpl + '_>, RangeOutOfBounds>;
    fn chunks(&self, range: Range) -> Result<Box<dyn ChunksIterImpl<'_> + '_>, RangeOutOfBounds>;
    fn chunks_mut(
        &mut self,
        range: Range,
    ) -> Result<Box<dyn ChunksMutIterImpl<'_> + '_>, RangeOutOfBounds>;
    fn reserve(&mut self, size: usize) -> Result<(), Full>;
    fn grow(&mut self, new_len: usize, value: u8) -> Result<(), Full>;
    fn extend(&mut self, with: &[u8]) -> Result<(), Full>;
    fn size_limit(&self) -> SizeLimit;
    fn split_at(
        self,
        at: usize,
    ) -> Result<(Box<dyn BytesMutImpl>, Box<dyn BytesMutImpl>), IndexOutOfBounds>;
}

pub trait ChunksIterImpl<'a>: Iterator<Item = &'a [u8]> + DoubleEndedIterator {}

impl<'a, T: Iterator<Item = &'a [u8]> + DoubleEndedIterator> ChunksIterImpl<'a> for T {}

pub trait ChunksMutIterImpl<'a>: Iterator<Item = &'a mut [u8]> + DoubleEndedIterator {}

impl<'a, T: Iterator<Item = &'a mut [u8]> + DoubleEndedIterator> ChunksMutIterImpl<'a> for T {}

pub trait BytesMutViewImpl<'b>: Length {
    fn view(&self, range: Range) -> Result<Box<dyn BytesMutViewImpl<'b> + 'b>, RangeOutOfBounds>;
    fn chunks<'a>(
        &'a self,
        range: Range,
    ) -> Result<Box<dyn ChunksIterImpl<'a> + 'a>, RangeOutOfBounds>;
}

pub trait BytesMutViewMutImpl<'b>: Length {
    fn view(&self, range: Range) -> Result<Box<dyn BytesMutViewImpl + '_>, RangeOutOfBounds>;
    fn view_mut(
        &mut self,
        range: Range,
    ) -> Result<Box<dyn BytesMutViewMutImpl + '_>, RangeOutOfBounds>;
    fn chunks(&self, range: Range) -> Result<Box<dyn ChunksIterImpl<'_> + '_>, RangeOutOfBounds>;
    fn chunks_mut(
        &mut self,
        range: Range,
    ) -> Result<Box<dyn ChunksMutIterImpl<'_> + '_>, RangeOutOfBounds>;
    fn reserve(&mut self, size: usize) -> Result<(), Full>;
    fn grow(&mut self, new_len: usize, value: u8) -> Result<(), Full>;
    fn extend(&mut self, with: &[u8]) -> Result<(), Full>;
    fn size_limit(&self) -> SizeLimit;
    fn split_at(
        self,
        at: usize,
    ) -> Result<
        (
            Box<dyn BytesMutViewMutImpl<'b> + 'b>,
            Box<dyn BytesMutViewMutImpl<'b> + 'b>,
        ),
        IndexOutOfBounds,
    >;
}

impl BytesImpl for &'static [u8] {
    fn view(&self, range: Range) -> Result<Box<dyn BytesImpl>, RangeOutOfBounds> {
        Ok(Box::new(Buf::view(self, range)?))
    }

    fn chunks(&self, range: Range) -> Result<Box<dyn ChunksIterImpl<'_> + '_>, RangeOutOfBounds> {
        Ok(Box::new(Buf::chunks(self, range)?))
    }

    fn clone(&self) -> Box<dyn BytesImpl> {
        Box::new(*self)
    }
}

impl<'b> BytesMutViewImpl<'b> for &'b [u8] {
    fn view(&self, range: Range) -> Result<Box<dyn BytesMutViewImpl<'b> + 'b>, RangeOutOfBounds> {
        Ok(Box::new(Buf::view(self, range)?))
    }

    fn chunks(&self, range: Range) -> Result<Box<dyn ChunksIterImpl<'_> + '_>, RangeOutOfBounds> {
        Ok(Box::new(Buf::chunks(self, range)?))
    }
}

impl<'b> BytesMutViewMutImpl<'b> for &'b mut [u8] {
    fn view(&self, range: Range) -> Result<Box<dyn BytesMutViewImpl + '_>, RangeOutOfBounds> {
        Ok(Box::new(Buf::view(self, range)?))
    }

    fn view_mut(
        &mut self,
        range: Range,
    ) -> Result<Box<dyn BytesMutViewMutImpl + '_>, RangeOutOfBounds> {
        Ok(Box::new(BufMut::view_mut(self, range)?))
    }

    fn chunks(&self, range: Range) -> Result<Box<dyn ChunksIterImpl<'_> + '_>, RangeOutOfBounds> {
        Ok(Box::new(Buf::chunks(self, range)?))
    }

    fn chunks_mut(
        &mut self,
        range: Range,
    ) -> Result<Box<dyn ChunksMutIterImpl<'_> + '_>, RangeOutOfBounds> {
        Ok(Box::new(BufMut::chunks_mut(self, range)?))
    }

    fn reserve(&mut self, size: usize) -> Result<(), Full> {
        BufMut::reserve(self, size)
    }

    fn grow(&mut self, new_len: usize, value: u8) -> Result<(), Full> {
        BufMut::grow(self, new_len, value)
    }

    fn extend(&mut self, with: &[u8]) -> Result<(), Full> {
        BufMut::extend(self, with)
    }

    fn size_limit(&self) -> SizeLimit {
        BufMut::size_limit(self)
    }

    fn split_at(
        self,
        at: usize,
    ) -> Result<
        (
            Box<dyn BytesMutViewMutImpl<'b> + 'b>,
            Box<dyn BytesMutViewMutImpl<'b> + 'b>,
        ),
        IndexOutOfBounds,
    > {
        if at <= self.len() {
            let (left, right) = (*self).split_at_mut(at);
            Ok((Box::new(left), Box::new(right)))
        }
        else {
            Err(IndexOutOfBounds {
                required: at,
                bounds: (0, self.len()),
            })
        }
    }
}
