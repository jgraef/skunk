pub mod arc_buf;
pub mod array_buf;
pub mod chunks;
mod empty;
mod partially_initialized;
pub mod rope;
mod slab;

use std::{
    borrow::Cow,
    fmt::Debug,
    ops::{
        Deref,
        DerefMut,
    },
    rc::Rc,
    sync::Arc,
};

use chunks::BufIter;

pub use self::{
    empty::Empty,
    slab::Slab,
};
use super::range::{
    Range,
    RangeOutOfBounds,
};
use crate::{
    impl_me,
    io::{
        BufReader,
        BufWriter,
        End,
    },
};

pub trait Length {
    /// Returns the length of this buffer in bytes.
    fn len(&self) -> usize;

    /// Returns whether this buffer is empty (i.e. has length 0).
    #[inline]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Read access to a buffer of bytes.
pub trait Buf: Length {
    /// A view of a portion of the buffer.
    type View<'a>: Buf + Sized + 'a
    where
        Self: 'a;

    type Reader<'a>: BufReader<View = Self::View<'a>>
    where
        Self: 'a;

    /// Returns a view of a portion of the buffer.
    fn view(&self, range: impl Into<Range>) -> Result<Self::View<'_>, RangeOutOfBounds>;

    /// Returns a [`BufReader`] for this buffer.
    fn reader(&self) -> Self::Reader<'_>;

    /// Returns whether this buffer contains bytes for the given range.
    ///
    /// # Default implementation
    ///
    /// The default implementation will check if the range is contained by
    /// `..self.len()`.
    #[inline]
    fn contains(&self, range: impl Into<Range>) -> bool {
        range.into().contained_by(..self.len())
    }
}

pub trait BufExt: Buf {
    #[inline]
    fn bytes_iter(&self) -> BufIter<'_, Self> {
        BufIter::new(self)
    }

    #[inline]
    fn into_vec(&self) -> Vec<u8> {
        let mut reader = self.reader();
        let mut buf = Vec::with_capacity(reader.remaining());
        while let Ok(chunk) = reader.chunk() {
            buf.extend(chunk.iter().copied());
            reader.advance(chunk.len()).unwrap();
        }
        buf
    }
}

impl<B: Buf> BufExt for B {}

/// Write access to a buffer of bytes.
pub trait BufMut: Buf {
    /// Mutable view of a portion of the buffer.
    type ViewMut<'a>: BufMut + Sized
    where
        Self: 'a;

    type Writer<'a>: BufWriter
    where
        Self: 'a;

    /// Returns a mutable view of a portion of the buffer.
    fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds>;

    fn writer(&mut self) -> Self::Writer<'_>;

    fn reserve(&mut self, size: usize) -> Result<(), Full>;

    fn size_limit(&self) -> SizeLimit;
}

#[derive(Clone, Copy, Debug, Default)]
pub enum SizeLimit {
    /// The [`BufMut`] can grow, but might get full.
    #[default]
    Unknown,

    /// The [`BufMut`] can grow limitless.
    Unlimited,

    /// The [`BufMut`] can grow to this exact length.
    Exact(usize),
}

impl From<usize> for SizeLimit {
    #[inline]
    fn from(value: usize) -> Self {
        Self::Exact(value)
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "Buffer is full: data with length ({required}) can't fit into buffer with length {capacity}."
)]
pub struct Full {
    pub required: usize,
    pub capacity: usize,
}

impl From<crate::io::Full> for Full {
    fn from(value: crate::io::Full) -> Self {
        Self {
            required: value.requested,
            capacity: value.remaining + value.written,
        }
    }
}

impl Length for [u8] {
    #[inline]
    fn len(&self) -> usize {
        <[u8]>::len(self)
    }
}

impl<const N: usize> Length for [u8; N] {
    #[inline]
    fn len(&self) -> usize {
        N
    }
}

impl<'a, T: Length + ?Sized> Length for &'a T {
    #[inline]
    fn len(&self) -> usize {
        T::len(self)
    }
}

impl<'a, T: Length + ?Sized> Length for &'a mut T {
    #[inline]
    fn len(&self) -> usize {
        T::len(self)
    }
}

impl<'a, T: Length + ?Sized> Length for Box<T> {
    #[inline]
    fn len(&self) -> usize {
        T::len(self)
    }
}

impl<'a, T: Length + ?Sized> Length for Arc<T> {
    #[inline]
    fn len(&self) -> usize {
        T::len(self)
    }
}

impl<'a, T: Length + ?Sized> Length for Rc<T> {
    #[inline]
    fn len(&self) -> usize {
        T::len(self)
    }
}

impl<'a, T: Length + ToOwned + ?Sized> Length for Cow<'a, T> {
    #[inline]
    fn len(&self) -> usize {
        T::len(self)
    }
}

impl<'a> Length for Vec<u8> {
    #[inline]
    fn len(&self) -> usize {
        Vec::len(self)
    }
}

macro_rules! impl_buf_with_deref {
    {
        $(
            ($($generics:tt)*), $ty:ty;
        )*
    } => {
        $(
            impl<$($generics)*> Buf for $ty {
                type View<'a> = <B as Buf>::View<'a> where Self: 'a;
                type Reader<'a> = <B as Buf>::Reader<'a> where Self: 'a;

                #[inline]
                fn view(&self, range: impl Into<Range>) -> Result<Self::View<'_>, RangeOutOfBounds> {
                    <B as Buf>::view(self.deref(), range)
                }

                #[inline]
                fn reader(&self) -> Self::Reader<'_> {
                    <B as Buf>::reader(self.deref())
                }
            }
        )*
    };
}

impl_buf_with_deref! {
    ('b, B: Buf + ?Sized), &'b B;
    ('b, B: Buf + ?Sized), &'b mut B;
    (B: Buf + ?Sized), Box<B>;
    (B: Buf + ?Sized), Arc<B>;
    (B: Buf + ?Sized), Rc<B>;
}

macro_rules! impl_buf_for_slice_like {
    {
        $(
            ($($generics:tt)*), $ty:ty, $view_lt:lifetime;
        )*
    } => {
        $(
            impl<$($generics)*> Buf for $ty {
                type View<'a> = & $view_lt [u8] where Self: 'a;
                type Reader<'a> = & $view_lt [u8] where Self: 'a;

                #[inline]
                fn view<'a>(&'a self, range: impl Into<Range>) -> Result<& $view_lt [u8], RangeOutOfBounds> {
                    range.into().slice_get(self)
                }

                #[inline]
                fn reader(&self) -> Self::Reader<'_> {
                    self
                }
            }
        )*
    };
}

// note: it would be better to impl `Buf` for `[u8]` and let the blanket impls
// above impl for `&[u8]` etc., but an implementation for `[u8]` would have
// `Buf::View = &[u8]`, which at that point doesn't implement `Buf` yet. it's
// the classic chicken-egg problem.
impl_buf_for_slice_like! {
    ('b), &'b [u8], 'b;
    (const N: usize), [u8; N], 'a;
    ('b), &'b mut [u8], 'a;
    (), Vec<u8>, 'a;
    (), Box<[u8]>, 'a;
    (), Arc<[u8]>, 'a;
    ('b), Cow<'b, [u8]>, 'a;
}

impl<'b, B: BufMut + ?Sized> BufMut for &'b mut B {
    type ViewMut<'a> = <B as BufMut>::ViewMut<'a> where Self: 'a;
    type Writer<'a> = <B as BufMut>::Writer<'a> where Self: 'a;

    #[inline]
    fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds> {
        <B as BufMut>::view_mut(self.deref_mut(), range)
    }

    #[inline]
    fn writer(&mut self) -> Self::Writer<'_> {
        <B as BufMut>::writer(self.deref_mut())
    }

    #[inline]
    fn reserve(&mut self, size: usize) -> Result<(), Full> {
        <B as BufMut>::reserve(self.deref_mut(), size)
    }

    #[inline]
    fn size_limit(&self) -> SizeLimit {
        <B as BufMut>::size_limit(self)
    }
}

impl<B: BufMut + ?Sized> BufMut for Box<B> {
    type ViewMut<'a> = <B as BufMut>::ViewMut<'a> where Self: 'a;
    type Writer<'a> = <B as BufMut>::Writer<'a> where Self: 'a;

    #[inline]
    fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds> {
        <B as BufMut>::view_mut(self.deref_mut(), range)
    }

    #[inline]
    fn writer(&mut self) -> Self::Writer<'_> {
        <B as BufMut>::writer(self.deref_mut())
    }

    #[inline]
    fn reserve(&mut self, size: usize) -> Result<(), Full> {
        <B as BufMut>::reserve(self.deref_mut(), size)
    }

    #[inline]
    fn size_limit(&self) -> SizeLimit {
        <B as BufMut>::size_limit(self)
    }
}

impl<'b> BufMut for &'b mut [u8] {
    type ViewMut<'a> = &'a mut [u8] where Self: 'a;
    type Writer<'a> = &'a mut [u8] where Self: 'a;

    #[inline]
    fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds> {
        range.into().slice_get_mut(self)
    }

    #[inline]
    fn writer(&mut self) -> Self::Writer<'_> {
        self
    }

    #[inline]
    fn reserve(&mut self, size: usize) -> Result<(), Full> {
        if size > self.len() {
            Err(Full {
                required: size,
                capacity: self.len(),
            })
        }
        else {
            Ok(())
        }
    }

    #[inline]
    fn size_limit(&self) -> SizeLimit {
        self.len().into()
    }
}

impl<const N: usize> BufMut for [u8; N] {
    type ViewMut<'a> = &'a mut [u8] where Self: 'a;
    type Writer<'a> = &'a mut [u8] where Self: 'a;

    #[inline]
    fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds> {
        range.into().slice_get_mut(self)
    }

    #[inline]
    fn writer(&mut self) -> Self::Writer<'_> {
        self
    }

    #[inline]
    fn reserve(&mut self, size: usize) -> Result<(), Full> {
        if size > N {
            Err(Full {
                required: size,
                capacity: self.len(),
            })
        }
        else {
            Ok(())
        }
    }

    #[inline]
    fn size_limit(&self) -> SizeLimit {
        self.len().into()
    }
}

impl BufMut for Box<[u8]> {
    type ViewMut<'a> = &'a mut [u8] where Self: 'a;
    type Writer<'a> = &'a mut [u8] where Self: 'a;

    #[inline]
    fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds> {
        range.into().slice_get_mut(self)
    }

    #[inline]
    fn writer(&mut self) -> Self::Writer<'_> {
        self
    }

    #[inline]
    fn reserve(&mut self, size: usize) -> Result<(), Full> {
        if size > self.len() {
            Err(Full {
                required: size,
                capacity: self.len(),
            })
        }
        else {
            Ok(())
        }
    }

    #[inline]
    fn size_limit(&self) -> SizeLimit {
        self.len().into()
    }
}

impl BufMut for Vec<u8> {
    type ViewMut<'a> = &'a mut [u8] where Self: 'a;
    type Writer<'a> = VecWriter<'a> where Self: 'a;

    #[inline]
    fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds> {
        range.into().slice_get_mut(self)
    }

    #[inline]
    fn writer(&mut self) -> Self::Writer<'_> {
        VecWriter::new(self)
    }

    #[inline]
    fn reserve(&mut self, size: usize) -> Result<(), Full> {
        if size > self.len() {
            self.reserve_exact(size - self.len());
        }
        Ok(())
    }

    #[inline]
    fn size_limit(&self) -> SizeLimit {
        SizeLimit::Unlimited
    }
}

#[derive(Debug)]
pub struct VecWriter<'a> {
    vec: &'a mut Vec<u8>,
    position: usize,
}

impl<'a> VecWriter<'a> {
    #[inline]
    fn new(vec: &'a mut Vec<u8>) -> Self {
        Self { vec, position: 0 }
    }
}

impl<'a> BufWriter for VecWriter<'a> {
    #[inline]
    fn chunk_mut(&mut self) -> Result<&mut [u8], End> {
        (self.position < self.vec.len())
            .then(|| &mut self.vec[self.position..])
            .ok_or(End)
    }

    #[inline]
    fn advance(&mut self, by: usize) -> Result<(), crate::io::Full> {
        let n = (self.position + by).saturating_sub(self.vec.len());
        self.vec.extend((0..n).into_iter().map(|_| 0));
        self.position += by;
        Ok(())
    }

    #[inline]
    fn remaining(&self) -> usize {
        self.vec.len()
    }

    #[inline]
    fn extend(&mut self, with: &[u8]) -> Result<(), crate::io::Full> {
        let n_overwrite = std::cmp::min(self.vec.len() - self.position, with.len());
        self.vec[self.position..][..n_overwrite].copy_from_slice(&with[..n_overwrite]);
        self.vec.extend(with[n_overwrite..].iter().copied());
        self.position += with.len();
        Ok(())
    }
}

impl_me! {
    impl['a] Writer for VecWriter<'a> as BufWriter;
}

#[cfg(test)]
pub(crate) mod tests {
    macro_rules! buf_mut_tests {
        ($new:expr) => {
            #[test]
            fn copy_with_fill() {
                use ::byst::{
                    buf::{
                        Buf as _,
                        BufReader as _,
                    },
                    copy_range,
                };
                let mut bytes_mut = $new;
                copy_range(&mut bytes_mut, 4..8, b"abcd", ..).unwrap();
                assert_eq!(bytes_mut.reader().chunk().unwrap(), b"\x00\x00\x00\x00abcd");
            }

            #[test]
            fn copy_over_buf_end() {
                use ::byst::{
                    buf::{
                        Buf as _,
                        BufReader as _,
                    },
                    copy_range,
                };
                let mut bytes_mut = $new;
                copy_range(&mut bytes_mut, 0..4, b"abcd", ..).unwrap();
                copy_range(&mut bytes_mut, 2..6, b"efgh", ..).unwrap();
                assert_eq!(bytes_mut.reader().chunk().unwrap(), b"abefgh");
            }

            #[test]
            fn copy_extend_with_unbounded_destination_slice() {
                use ::byst::{
                    buf::{
                        Buf as _,
                        BufReader as _,
                    },
                    copy_range,
                };
                let mut bytes_mut = $new;
                copy_range(&mut bytes_mut, 0..4, b"abcd", ..).unwrap();
                copy_range(&mut bytes_mut, 2.., b"efgh", ..).unwrap();
                assert_eq!(bytes_mut.reader().chunk().unwrap(), b"abefgh");
            }
        };
    }
    pub(crate) use buf_mut_tests;

    mod vec {
        buf_mut_tests!(Vec::<u8>::new());
    }
}
