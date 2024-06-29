use std::fmt::Debug;

use super::r#impl::{
    BytesImpl,
    BytesMutImpl,
    WriterImpl,
};
use crate::{
    buf::{
        Empty,
        Full,
        Length,
        SizeLimit,
    },
    impl_me,
    io::{
        BufReader,
        BufWriter,
        End,
        Seek,
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

pub struct View<'b> {
    inner: Box<dyn BytesImpl<'b> + 'b>,
}

impl<'b> View<'b> {
    cfg_pub! {
        #[inline]
        pub(#[cfg(feature = "bytes-impl")]) fn from_impl(inner: Box<dyn BytesImpl<'b> + 'b>) -> Self {
            Self { inner }
        }
    }
}

impl<'b> Default for View<'b> {
    fn default() -> Self {
        Self::from_impl(Box::new(Empty))
    }
}

impl<'b> Clone for View<'b> {
    #[inline]
    fn clone(&self) -> Self {
        Self::from_impl(self.inner.clone())
    }
}

impl<'b> Debug for View<'b> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        debug_as_hexdump(f, self)
    }
}

impl<'b, R: Buf> PartialEq<R> for View<'b> {
    #[inline]
    fn eq(&self, other: &R) -> bool {
        buf_eq(self, other)
    }
}

impl<'b> Length for View<'b> {
    #[inline]
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<'b> Buf for View<'b> {
    type View<'a> = Self
    where
        Self: 'a;

    type Reader<'a> = Self
        where
            Self: 'a;

    #[inline]
    fn view(&self, range: impl Into<Range>) -> Result<Self::View<'_>, RangeOutOfBounds> {
        Ok(View::from_impl(self.inner.view(range.into())?))
    }

    #[inline]
    fn reader(&self) -> Self::Reader<'_> {
        self.clone()
    }
}

impl<'b> BufReader for View<'b> {
    type View = Self;

    #[inline]
    fn peek_chunk(&self) -> Option<&[u8]> {
        self.inner.peek_chunk()
    }

    #[inline]
    fn view(&mut self, length: usize) -> Result<Self::View, End> {
        let view = self.peek_view(length)?;
        self.inner
            .advance(length)
            .unwrap_or_else(|_| unreachable!());
        Ok(view)
    }

    #[inline]
    fn peek_view(&self, length: usize) -> Result<Self, End> {
        Buf::view(self, Range::default().with_start(0).with_length(length)).map_err(|_| {
            End {
                read: 0,
                requested: length,
                remaining: self.inner.len(),
            }
        })
    }

    #[inline]
    fn rest(&mut self) -> Self::View {
        std::mem::take(self)
    }

    #[inline]
    fn peek_rest(&self) -> Self::View {
        self.clone()
    }

    #[inline]
    fn advance(&mut self, by: usize) -> Result<(), End> {
        self.inner.advance(by)
    }

    #[inline]
    fn remaining(&self) -> usize {
        self.inner.len()
    }
}

impl<'b> Seek for View<'b> {
    type Position = View<'b>;

    #[inline]
    fn tell(&self) -> Self::Position {
        self.clone()
    }

    #[inline]
    fn seek(&mut self, position: &Self::Position) -> Self::Position {
        std::mem::replace(self, position.clone())
    }
}

pub struct ViewMut<'b> {
    inner: Box<dyn BytesMutImpl + 'b>,
}

impl<'b> ViewMut<'b> {
    cfg_pub! {
        #[inline]
        pub(#[cfg(feature = "bytes-impl")]) fn from_impl(inner: Box<dyn BytesMutImpl + 'b>) -> Self {
            Self { inner }
        }
    }
}

impl<'b> Debug for ViewMut<'b> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        debug_as_hexdump(f, self)
    }
}

impl<'b, R: Buf> PartialEq<R> for ViewMut<'b> {
    #[inline]
    fn eq(&self, other: &R) -> bool {
        buf_eq(self, other)
    }
}

impl<'b> Length for ViewMut<'b> {
    #[inline]
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<'b> Buf for ViewMut<'b> {
    type View<'a> = View<'a>
    where
        Self: 'a;

    type Reader<'a> = View<'a>
        where
            Self: 'a;

    #[inline]
    fn view(&self, range: impl Into<Range>) -> Result<Self::View<'_>, RangeOutOfBounds> {
        Ok(View::from_impl(self.inner.view(range.into())?))
    }

    #[inline]
    fn reader(&self) -> Self::Reader<'_> {
        View::from_impl(self.inner.reader())
    }
}

impl<'b> BufMut for ViewMut<'b> {
    type ViewMut<'a> = ViewMut<'a>
    where
        Self: 'a;

    type Writer<'a> = ViewMutWriter<'a>
        where
            Self: 'a;

    #[inline]
    fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds> {
        Ok(ViewMut::from_impl(self.inner.view_mut(range.into())?))
    }

    #[inline]
    fn writer(&mut self) -> Self::Writer<'_> {
        ViewMutWriter::from_impl(self.inner.writer())
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

pub struct ViewMutWriter<'b> {
    inner: Box<dyn WriterImpl + 'b>,
}

impl<'b> ViewMutWriter<'b> {
    cfg_pub! {
        #[inline]
        pub(#[cfg(feature = "bytes-impl")]) fn from_impl(inner: Box<dyn WriterImpl + 'b>) -> Self {
            Self { inner }
        }
    }
}

impl<'b> BufWriter for ViewMutWriter<'b> {
    type ViewMut<'a> = ViewMut<'a> where Self: 'a;

    fn peek_chunk_mut(&mut self) -> Option<&mut [u8]> {
        self.inner.peek_chunk_mut()
    }

    fn view_mut(&mut self, _length: usize) -> Result<Self::ViewMut<'_>, crate::io::Full> {
        todo!();
    }

    #[inline]
    fn peek_view_mut(&mut self, _length: usize) -> Result<Self::ViewMut<'_>, crate::io::Full> {
        todo!();
    }

    #[inline]
    fn rest_mut(&mut self) -> Self::ViewMut<'_> {
        todo!();
    }

    #[inline]
    fn peek_rest_mut(&mut self) -> Self::ViewMut<'_> {
        todo!();
    }

    fn advance(&mut self, by: usize) -> Result<(), crate::io::Full> {
        self.inner.advance(by)
    }

    fn remaining(&self) -> usize {
        self.inner.remaining()
    }

    fn extend(&mut self, with: &[u8]) -> Result<(), crate::io::Full> {
        self.inner.extend(with)
    }
}

impl_me! {
    impl['a] Reader for View<'a> as BufReader;
    impl['a] Read<_, ()> for View<'a> as BufReader::View;
    impl['a] Writer for ViewMutWriter<'a> as BufWriter;
}
