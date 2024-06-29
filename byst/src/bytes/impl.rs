#![allow(dead_code)]

use crate::{
    buf::{
        Full,
        Length,
        SizeLimit,
    },
    io::{BufReader, BufWriter, End},
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
pub trait BytesImpl<'b>: Length + Send + Sync {
    fn clone(&self) -> Box<dyn BytesImpl<'b> + 'b>;
    fn peek_chunk(&self) -> Option<&[u8]>;
    fn view(&self, range: Range) -> Result<Box<dyn BytesImpl<'b> + 'b>, RangeOutOfBounds>;
    fn advance(&mut self, by: usize) -> Result<(), End>;
}

/// The trait backing the [`BytesMut`] implementation.
///
/// Implement this for your type, for it to be usable as a [`BytesMut`]. Use
/// [`BytesMut::from_impl`] to implement a conversion from your type to
/// [`BytesMut`].
///
/// [`BytesMut`]: super::BytesMut
/// [`BytesMut::from_impl`]: super::BytesMut::from_impl
pub trait BytesMutImpl: Length + Send + Sync {
    fn view(&self, range: Range) -> Result<Box<dyn BytesImpl<'_> + '_>, RangeOutOfBounds>;
    fn view_mut(&mut self, range: Range) -> Result<Box<dyn BytesMutImpl + '_>, RangeOutOfBounds>;
    fn reader(&self) -> Box<dyn BytesImpl<'_> + '_>;
    fn writer(&mut self) -> Box<dyn WriterImpl + '_>;
    fn reserve(&mut self, size: usize) -> Result<(), Full>;
    fn size_limit(&self) -> SizeLimit;
    fn split_at(
        &mut self,
        at: usize,
    ) -> Result<Box<dyn BytesMutImpl + '_>, IndexOutOfBounds>;
}

pub trait WriterImpl {
    fn peek_chunk_mut(&mut self) -> Option<&mut [u8]>;
    fn advance(&mut self, by: usize) -> Result<(), crate::io::Full>;
    fn remaining(&self) -> usize;
    fn extend(&mut self, with: &[u8]) -> Result<(), crate::io::Full>;
}

impl<'b> BytesImpl<'b> for &'b [u8] {
    fn view(&self, range: Range) -> Result<Box<dyn BytesImpl<'b> + 'b>, RangeOutOfBounds> {
        Ok(Box::new(Buf::view(self, range)?))
    }

    fn clone(&self) -> Box<dyn BytesImpl<'b> + 'b> {
        Box::new(*self)
    }

    fn peek_chunk(&self) -> Option<&[u8]> {
        BufReader::peek_chunk(self)
    }

    fn advance(&mut self, by: usize) -> Result<(), End> {
        BufReader::advance(self, by)
    }
}

impl<'b> BytesMutImpl for &'b mut [u8] {
    fn view(&self, range: Range) -> Result<Box<dyn BytesImpl + '_>, RangeOutOfBounds> {
        Ok(Box::new(Buf::view(self, range)?))
    }

    fn view_mut(&mut self, range: Range) -> Result<Box<dyn BytesMutImpl + '_>, RangeOutOfBounds> {
        Ok(Box::new(BufMut::view_mut(self, range)?))
    }

    fn reader(&self) -> Box<dyn BytesImpl<'_> + '_> {
        Box::new(&**self)
    }

    fn writer(&mut self) -> Box<dyn WriterImpl + '_> {
        Box::new(&mut **self)
    }

    fn reserve(&mut self, size: usize) -> Result<(), Full> {
        BufMut::reserve(self, size)
    }

    fn size_limit(&self) -> SizeLimit {
        BufMut::size_limit(self)
    }

    fn split_at(
        &mut self,
        at: usize,
    ) -> Result<Box<dyn BytesMutImpl + '_>, IndexOutOfBounds> {
        let (left, right) = <[u8]>::split_at_mut(std::mem::take(self), at);
        *self = right;
        Ok(Box::new(left))
    }
}

impl<'b> WriterImpl for &'b mut [u8] {
    fn peek_chunk_mut(&mut self) -> Option<&mut [u8]> {
        BufWriter::peek_chunk_mut(self)
    }

    fn advance(&mut self, by: usize) -> Result<(), crate::io::Full> {
        BufWriter::advance(self, by)
    }

    fn remaining(&self) -> usize {
        <[u8]>::len(self)
    }

    fn extend(&mut self, with: &[u8]) -> Result<(), crate::io::Full> {
        BufWriter::extend(self, with)
    }
}
