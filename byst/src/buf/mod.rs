pub mod arc_buf;
mod array_buf;
pub mod chunks;
pub mod copy;
mod empty;
mod partially_initialized;
pub mod rope;

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

use self::chunks::{
    BufIter,
    BufIterMut,
    SingleChunk,
    SingleChunkMut,
};
pub use self::{
    array_buf::ArrayBuf,
    empty::Empty,
};
use super::range::{
    Range,
    RangeOutOfBounds,
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

impl Length for [u8] {
    fn len(&self) -> usize {
        <[u8]>::len(self)
    }
}

impl<const N: usize> Length for [u8; N] {
    fn len(&self) -> usize {
        N
    }
}

impl<'a, T: Length + ?Sized> Length for &'a T {
    fn len(&self) -> usize {
        T::len(self)
    }
}

impl<'a, T: Length + ?Sized> Length for &'a mut T {
    fn len(&self) -> usize {
        T::len(self)
    }
}

impl<'a, T: Length + ?Sized> Length for Box<T> {
    fn len(&self) -> usize {
        T::len(self)
    }
}

impl<'a, T: Length + ?Sized> Length for Arc<T> {
    fn len(&self) -> usize {
        T::len(self)
    }
}

impl<'a, T: Length + ?Sized> Length for Rc<T> {
    fn len(&self) -> usize {
        T::len(self)
    }
}

impl<'a, T: Length + ToOwned + ?Sized> Length for Cow<'a, T> {
    fn len(&self) -> usize {
        T::len(self)
    }
}

impl<'a> Length for Vec<u8> {
    fn len(&self) -> usize {
        Vec::len(self)
    }
}

/// Read access to a buffer of bytes.
///
/// # TODO
///
/// - Some methods have the same name as methods in `[u8]`, which is annoying
///   when trying to use them in a `&[u8]`.
pub trait Buf: Length {
    /// A view of a portion of the buffer.
    type View<'a>: Buf + Sized + 'a
    where
        Self: 'a;

    /// Iterator over contiguous byte chunks that make up this buffer.
    type Chunks<'a>: Iterator<Item = &'a [u8]>
    where
        Self: 'a;

    /// Returns a view of a portion of the buffer.
    fn view(&self, range: impl Into<Range>) -> Result<Self::View<'_>, RangeOutOfBounds>;

    /// Returns an iterator over contiguous byte chunks that make up this
    /// buffer.
    fn chunks(&self, range: impl Into<Range>) -> Result<Self::Chunks<'_>, RangeOutOfBounds>;

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

    #[inline]
    fn iter(&self, range: impl Into<Range>) -> Result<BufIter<'_, Self>, RangeOutOfBounds> {
        let range = range.into();
        let len = range.len_in(0, self.len());
        Ok(BufIter::new(self.chunks(range)?, len))
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
                type Chunks<'a> = <B as Buf>::Chunks<'a> where Self: 'a;

                #[inline]
                fn view(&self, range: impl Into<Range>) -> Result<Self::View<'_>, RangeOutOfBounds> {
                    <B as Buf>::view(self.deref(), range)
                }

                #[inline]
                fn chunks(&self, range: impl Into<Range>) -> Result<Self::Chunks<'_>, RangeOutOfBounds> {
                    <B as Buf>::chunks(self.deref(), range)
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
                type View<'a> = &$view_lt [u8] where Self: 'a;

                type Chunks<'a> = SingleChunk<'a> where Self: 'a;

                #[inline]
                fn view<'a>(&'a self, range: impl Into<Range>) -> Result<Self::View<$view_lt>, RangeOutOfBounds> {
                    range.into().slice_get(self)
                }

                #[inline]
                fn chunks<'a>(&'a self, range: impl Into<Range>) -> Result<Self::Chunks<$view_lt>, RangeOutOfBounds> {
                    Ok(SingleChunk::new(range.into().slice_get(self)?))
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

/// Write access to a buffer of bytes.
pub trait BufMut: Buf {
    /// Mutable view of a portion of the buffer.
    type ViewMut<'a>: BufMut + Sized
    where
        Self: 'a;

    /// Iterator over contiguous byte chunks that make up this buffer.
    type ChunksMut<'a>: Iterator<Item = &'a mut [u8]>
    where
        Self: 'a;

    /// Returns a mutable view of a portion of the buffer.
    fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds>;

    /// Returns an iterator over contiguous mutable byte chunks that make up
    /// this buffer.
    fn chunks_mut(
        &mut self,
        range: impl Into<Range>,
    ) -> Result<Self::ChunksMut<'_>, RangeOutOfBounds>;

    /// Returns a mutable iterator over the bytes of this buffer.
    #[inline]
    fn iter_mut(
        &mut self,
        range: impl Into<Range>,
    ) -> Result<BufIterMut<'_, Self>, RangeOutOfBounds> {
        let range = range.into();
        let len = range.len_in(0, self.len());
        Ok(BufIterMut::new(self.chunks_mut(range)?, len))
    }

    fn reserve(&mut self, size: usize) -> Result<(), Full>;

    fn grow(&mut self, new_len: usize, value: u8) -> Result<(), Full>;

    fn extend(&mut self, with: &[u8]) -> Result<(), Full>;

    fn size_limit(&self) -> SizeLimit;
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "buffer is full: data with length ({required}) can't fit into buffer with length {capacity}"
)]
pub struct Full {
    pub required: usize,
    pub capacity: usize,
}

macro_rules! impl_buf_mut_with_deref {
    {
        $(
            ($($generics:tt)*), $ty:ty;
        )*
    } => {
        $(
            impl<$($generics)*> BufMut for $ty {
                type ViewMut<'a> = <B as BufMut>::ViewMut<'a> where Self: 'a;

                type ChunksMut<'a> = <B as BufMut>::ChunksMut<'a> where Self: 'a;

                #[inline]
                fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds> {
                    <B as BufMut>::view_mut(self.deref_mut(), range)
                }

                #[inline]
                fn chunks_mut(
                    &mut self,
                    range: impl Into<Range>,
                ) -> Result<Self::ChunksMut<'_>, RangeOutOfBounds> {
                    <B as BufMut>::chunks_mut(self.deref_mut(), range)
                }

                #[inline]
                fn reserve(&mut self, size: usize) -> Result<(), Full> {
                    <B as BufMut>::reserve(self.deref_mut(), size)
                }

                #[inline]
                #[allow(unused_variables)]
                fn grow(&mut self, new_len: usize, value: u8) -> Result<(), Full> {
                    <B as BufMut>::grow(self.deref_mut(), new_len, value)
                }

                #[inline]
                #[allow(unused_variables)]
                fn extend(&mut self, with: &[u8]) -> Result<(), Full> {
                    <B as BufMut>::extend(self.deref_mut(), with)
                }

                #[inline]
                fn size_limit(&self) -> SizeLimit {
                    <B as BufMut>::size_limit(self.deref())
                }
            }
        )*
    };
}

impl_buf_mut_with_deref! {
    ('b, B: BufMut + ?Sized), &'b mut B;
    (B: BufMut + ?Sized), Box<B>;
}

macro_rules! impl_buf_mut_for_slice_like {
    {
        $(
            ($($generics:tt)*), $ty:ty;
        )*
    } => {
        $(
            impl<$($generics)*> BufMut for $ty {
                type ViewMut<'a> = &'a mut [u8] where Self: 'a;

                type ChunksMut<'a> = SingleChunkMut<'a> where Self: 'a;

                #[inline]
                fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds> {
                    range.into().slice_get_mut(self)
                }

                #[inline]
                fn chunks_mut(
                    &mut self,
                    range: impl Into<Range>,
                ) -> Result<Self::ChunksMut<'_>, RangeOutOfBounds> {
                    Ok(SingleChunkMut::new(range.into().slice_get_mut(self)?))
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
                #[allow(unused_variables)]
                fn grow(&mut self, new_len: usize, value: u8) -> Result<(), Full> {
                    if new_len > self.len() {
                        Err(Full {
                            required: new_len,
                            capacity: self.len(),
                        })
                    }
                    else {
                        Ok(())
                    }
                }

                #[inline]
                #[allow(unused_variables)]
                fn extend(&mut self, with: &[u8]) -> Result<(), Full> {
                    Err(Full {
                        required: with.len(),
                        capacity: self.len(),
                    })
                }

                #[inline]
                fn size_limit(&self) -> SizeLimit {
                    self.len().into()
                }
            }
        )*
    };
}

impl_buf_mut_for_slice_like! {
    ('b), &'b mut [u8];
    (const N: usize), [u8; N];
    (), Box<[u8]>;
}

impl BufMut for Vec<u8> {
    type ViewMut<'a> = &'a mut [u8] where Self: 'a;

    type ChunksMut<'a> = SingleChunkMut<'a> where Self: 'a;

    #[inline]
    fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds> {
        range.into().slice_get_mut(self)
    }

    #[inline]
    fn chunks_mut(
        &mut self,
        range: impl Into<Range>,
    ) -> Result<Self::ChunksMut<'_>, RangeOutOfBounds> {
        Ok(SingleChunkMut::new(range.into().slice_get_mut(self)?))
    }

    #[inline]
    fn reserve(&mut self, size: usize) -> Result<(), Full> {
        if size > self.len() {
            self.reserve_exact(size - self.len());
        }
        Ok(())
    }

    #[inline]
    fn grow(&mut self, new_len: usize, value: u8) -> Result<(), Full> {
        if new_len > self.len() {
            self.resize(new_len, value);
        }
        Ok(())
    }

    #[inline]
    fn extend(&mut self, with: &[u8]) -> Result<(), Full> {
        Extend::extend(self, with);
        Ok(())
    }

    #[inline]
    fn size_limit(&self) -> SizeLimit {
        SizeLimit::Unlimited
    }
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

#[cfg(test)]
mod tests {
    mod vec {
        use crate::buf::{
            copy::copy,
            Buf,
        };

        #[test]
        fn copy_with_fill() {
            let mut bytes_mut = Vec::<u8>::new();
            copy(&mut bytes_mut, 4..8, b"abcd", ..).unwrap();
            assert_eq!(
                bytes_mut.chunks(..).unwrap().next().unwrap(),
                b"\x00\x00\x00\x00abcd"
            );
        }

        #[test]
        fn copy_over_buf_end() {
            let mut bytes_mut = Vec::<u8>::new();
            copy(&mut bytes_mut, 0..4, b"abcd", ..).unwrap();
            copy(&mut bytes_mut, 2..6, b"efgh", ..).unwrap();
            assert_eq!(bytes_mut.chunks(..).unwrap().next().unwrap(), b"abefgh");
        }

        #[test]
        fn copy_extend_with_unbounded_destination_slice() {
            let mut bytes_mut = Vec::<u8>::new();
            copy(&mut bytes_mut, 0..4, b"abcd", ..).unwrap();
            copy(&mut bytes_mut, 2.., b"efgh", ..).unwrap();
            assert_eq!(bytes_mut.chunks(..).unwrap().next().unwrap(), b"abefgh");
        }
    }
}
