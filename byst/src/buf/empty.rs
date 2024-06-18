use super::{
    Buf,
    Full,
    Length,
    SizeLimit,
};
use crate::{
    bytes::r#impl::{
        BytesImpl,
        BytesMutImpl,
        BytesMutViewImpl,
        BytesMutViewMutImpl,
        ChunksIterImpl,
    },
    BufMut,
    Bytes,
    IndexOutOfBounds,
    Range,
    RangeOutOfBounds,
};

/// An empty buffer.
#[derive(Debug, Clone, Copy, Default)]
pub struct Empty;

impl From<Empty> for Bytes {
    #[inline]
    fn from(value: Empty) -> Self {
        Self::from_impl(Box::new(value))
    }
}

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
}

impl BufMut for Empty {
    type ViewMut<'a> = Self
    where
        Self: 'a;

    type ChunksMut<'a> = std::iter::Empty<&'a mut [u8]>
    where
        Self: 'a;

    #[inline]
    fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds> {
        Buf::view(self, range)
    }

    #[inline]
    fn chunks_mut(
        &mut self,
        range: impl Into<Range>,
    ) -> Result<Self::ChunksMut<'_>, RangeOutOfBounds> {
        range.into().indices_checked_in(0, 0)?;
        Ok(std::iter::empty())
    }

    #[inline]
    fn reserve(&mut self, size: usize) -> Result<(), Full> {
        if size == 0 {
            Ok(())
        }
        else {
            Err(Full {
                required: size,
                capacity: 0,
            })
        }
    }

    #[inline]
    fn grow(&mut self, new_len: usize, _value: u8) -> Result<(), Full> {
        BufMut::reserve(self, new_len)
    }

    #[inline]
    fn extend(&mut self, with: &[u8]) -> Result<(), Full> {
        BufMut::reserve(self, with.len())
    }

    #[inline]
    fn size_limit(&self) -> SizeLimit {
        SizeLimit::Exact(0)
    }
}

impl Length for Empty {
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
    fn view(&self, range: Range) -> Result<Box<dyn BytesImpl>, RangeOutOfBounds> {
        Ok(Box::new(Buf::view(self, range)?))
    }

    fn chunks(&self, range: Range) -> Result<Box<dyn ChunksIterImpl<'_> + '_>, RangeOutOfBounds> {
        Ok(Box::new(Buf::chunks(self, range)?))
    }

    fn clone(&self) -> Box<dyn BytesImpl> {
        Box::new(Self)
    }
}

impl BytesMutImpl for Empty {
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
    ) -> Result<Box<dyn crate::bytes::r#impl::ChunksMutIterImpl<'_> + '_>, RangeOutOfBounds> {
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
    ) -> Result<(Box<dyn BytesMutImpl>, Box<dyn BytesMutImpl>), IndexOutOfBounds> {
        if at == 0 {
            Ok((Box::new(Self), Box::new(Self)))
        }
        else {
            Err(IndexOutOfBounds {
                required: at,
                bounds: (0, 0),
            })
        }
    }
}

impl<'b> BytesMutViewImpl<'b> for Empty {
    fn view(&self, range: Range) -> Result<Box<dyn BytesMutViewImpl<'b> + 'b>, RangeOutOfBounds> {
        Ok(Box::new(Buf::view(self, range)?))
    }

    fn chunks<'a>(
        &'a self,
        range: Range,
    ) -> Result<Box<dyn ChunksIterImpl<'a> + 'a>, RangeOutOfBounds> {
        Ok(Box::new(Buf::chunks(self, range)?))
    }
}

impl<'b> BytesMutViewMutImpl<'b> for Empty {
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
    ) -> Result<Box<dyn crate::bytes::r#impl::ChunksMutIterImpl<'_> + '_>, RangeOutOfBounds> {
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
        if at == 0 {
            Ok((Box::new(Self), Box::new(Self)))
        }
        else {
            Err(IndexOutOfBounds {
                required: at,
                bounds: (0, 0),
            })
        }
    }
}
