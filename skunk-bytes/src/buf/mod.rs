mod arc_buf;
mod array_buf;
pub mod chunks;
mod empty;
mod partially_initialized;

use std::{
    borrow::Cow,
    fmt::Debug,
    sync::Arc,
};

use self::chunks::{
    BufIter,
    BufIterMut,
    SingleChunk,
    SingleChunkMut,
};
pub use self::{
    arc_buf::ArcBuf,
    array_buf::ArrayBuf,
    empty::Empty,
};
use super::{
    copy::{
        copy,
        CopyError,
    },
    range::{
        Range,
        RangeOutOfBounds,
    },
};

/// Read access to a buffer of bytes.
pub trait Buf {
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

    /// Returns the length of this buffer in bytes.
    fn len(&self) -> usize;

    /// Returns whether this buffer is empty (i.e. has length 0).
    #[inline]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns whether this buffer contains bytes for the given range.
    ///
    /// # Default implementation
    ///
    /// The default implementation will check if the range is contained by
    /// `..self.len()`.
    fn contains(&self, range: impl Into<Range>) -> bool {
        range.into().contained_by(..self.len())
    }

    #[inline]
    fn iter(&self, range: impl Into<Range>) -> Result<BufIter<'_, Self>, RangeOutOfBounds> {
        Ok(BufIter::new(self.chunks(range)?))
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
                    <B as Buf>::view(*self, range)
                }

                #[inline]
                fn chunks(&self, range: impl Into<Range>) -> Result<Self::Chunks<'_>, RangeOutOfBounds> {
                    <B as Buf>::chunks(*self, range)
                }

                #[inline]
                fn len(&self) -> usize {
                    <B as Buf>::len(*self)
                }
            }
        )*
    };
}

impl_buf_with_deref! {
    ('b, B: Buf + ?Sized), &'b B;
    ('b, B: Buf + ?Sized), &'b mut B;
}

macro_rules! impl_buf_for_slice_like {
    {
        $(
            ($($generics:tt)*), $ty:ty, $view_lt:lifetime;
        )*
    } => {
        $(
            impl<$($generics)*> Buf for $ty {
                type View<'a> = &'a [u8] where Self: 'a;

                type Chunks<'a> = SingleChunk<'a> where Self: 'a;

                #[inline]
                fn view<'a>(&'a self, range: impl Into<Range>) -> Result<Self::View<$view_lt>, RangeOutOfBounds> {
                    range.into().slice_get(self)
                }

                #[inline]
                fn chunks<'a>(&'a self, range: impl Into<Range>) -> Result<Self::Chunks<$view_lt>, RangeOutOfBounds> {
                    Ok(SingleChunk::new(range.into().slice_get(self)?))
                }

                #[inline]
                fn len(&self) -> usize {
                    AsRef::<[u8]>::as_ref(self).len()
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
        Ok(BufIterMut::new(self.chunks_mut(range)?))
    }

    /// Grows the buffer such that it can hold the given range.
    ///
    /// # Default implementation
    ///
    /// The default implementation will check if the buffer can already hold the
    /// range, and fail if it can't.
    #[inline]
    fn grow_for(&mut self, range: impl Into<Range>) -> Result<(), RangeOutOfBounds> {
        let range = range.into();
        range
            .contained_by(0..self.len())
            .then_some(())
            .ok_or_else(|| {
                RangeOutOfBounds {
                    required: range,
                    bounds: (0, self.len()),
                }
            })
    }

    /// Writes the given buffer `source` into this one at `offset`, growing it
    /// as necessary
    ///
    /// # Default implementation
    ///
    /// The default implementation will first call [`Self::grow_for`] to make
    /// space and then copy to it. You should override it, if there is a a way
    /// to write the data without first allocating and initializing the space
    /// for it.
    #[inline]
    fn write(
        &mut self,
        destination_range: impl Into<Range>,
        source: impl Buf,
        source_range: impl Into<Range>,
    ) -> Result<(), WriteError> {
        let source_range = source_range.into();
        self.grow_for(source_range).map_err(|e| {
            WriteError::Full {
                required: e.required,
                buf_length: e.bounds.1,
            }
        })?;
        copy(self, destination_range, source, source_range)?;
        Ok(())
    }

    #[inline]
    fn size_limit(&self) -> SizeLimit {
        SizeLimit::Unknown
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum WriteError {
    #[error("buffer is full: range ({required:?}) can't fit into buffer with length {buf_length}")]
    Full { required: Range, buf_length: usize },

    #[error("{0}")]
    Copy(#[from] CopyError),
}

impl<'b, B: BufMut + ?Sized> BufMut for &'b mut B {
    type ViewMut<'a> = <B as BufMut>::ViewMut<'a> where Self: 'a;

    type ChunksMut<'a> = <B as BufMut>::ChunksMut<'a> where Self: 'a;

    #[inline]
    fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds> {
        <B as BufMut>::view_mut(*self, range)
    }

    #[inline]
    fn chunks_mut(
        &mut self,
        range: impl Into<Range>,
    ) -> Result<Self::ChunksMut<'_>, RangeOutOfBounds> {
        <B as BufMut>::chunks_mut(*self, range)
    }

    #[inline]
    fn size_limit(&self) -> SizeLimit {
        <B as BufMut>::size_limit(*self)
    }
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
    fn grow_for(&mut self, range: impl Into<Range>) -> Result<(), RangeOutOfBounds> {
        self.resize(range.into().len_in(0, self.len()), 0);
        Ok(())
    }

    fn write(
        &mut self,
        destination_range: impl Into<Range>,
        source: impl Buf,
        source_range: impl Into<Range>,
    ) -> Result<(), WriteError> {
        write_helper(
            self,
            destination_range,
            &source,
            source_range,
            |_, _| Ok(()),
            |this, n| this.reserve_exact(n),
            |this, n| this.resize(n, 0),
            |this, chunk| this.extend(chunk.iter().copied()),
        )
    }

    #[inline]
    fn size_limit(&self) -> SizeLimit {
        SizeLimit::Unlimited
    }
}

pub(super) fn write_helper<D: BufMut, S: Buf>(
    mut destination: &mut D,
    destination_range: impl Into<Range>,
    source: &S,
    source_range: impl Into<Range>,
    check_space: impl FnOnce(&D, usize) -> Result<(), usize>,
    reserve: impl FnOnce(&mut D, usize),
    fill_to: impl FnOnce(&mut D, usize),
    mut extend: impl FnMut(&mut D, &[u8]),
) -> Result<(), WriteError> {
    let source_range = source_range.into();
    let (source_start, source_end) = source_range.indices_unchecked_in(0, source.len());
    let source_range_length = source_end.saturating_sub(source_start);

    let destination_length = destination.len();
    let destination_range = destination_range.into();
    let destination_start = destination_range.start.unwrap_or_default();
    let (destination_end, destination_range_length) =
        if let Some(destination_end) = destination_range.end {
            let destination_range_length = destination_end.saturating_sub(destination_start);
            if destination_range_length != source_range_length {
                return Err(WriteError::Copy(CopyError::LengthMismatch {
                    destination_range,
                    destination_length,
                    source_range,
                    source_length: source.len(),
                }));
            }
            (destination_end, destination_range_length)
        }
        else {
            // if no upper bound for destination, we will write as much as needed to consume
            // the source range
            (destination_start + source_range_length, source_range_length)
        };

    if let Err(buf_length) = check_space(&destination, destination_end) {
        return Err(WriteError::Full {
            required: destination_range,
            buf_length,
        });
    }

    let mut part_written = 0;

    if destination_start < destination_length {
        // a portion is written by writing into the existing buffer
        part_written = source_start + destination.len() - destination_start;

        // todo: do this with [`copy_chunks`](super::copy_chunks), so we can use
        // [`BufMut::write`] to actually implement copy
        copy(
            &mut destination,
            destination_start..,
            &source,
            source_start..part_written,
        )
        .map_err(|e| {
            match e {
                CopyError::DestinationRangeOutOfBounds(e) => {
                    CopyError::DestinationRangeOutOfBounds(RangeOutOfBounds {
                        required: destination_range,
                        bounds: e.bounds,
                    })
                }
                CopyError::SourceRangeOutOfBounds(e) => {
                    CopyError::SourceRangeOutOfBounds(RangeOutOfBounds {
                        required: source_range,
                        bounds: e.bounds,
                    })
                }
                CopyError::LengthMismatch { .. } => {
                    // we already checked that
                    unreachable!()
                }
            }
        })?;
    }

    if destination_end > destination_length {
        // reserve space
        reserve(&mut destination, destination_end - destination_length);
    }

    if destination_start > destination_length {
        // the destination has to be filled with some zeros.
        fill_to(&mut destination, destination_range_length);
    }

    if destination_end > destination_length {
        // write rest to destination

        for chunk in source.chunks(part_written..source_end).unwrap() {
            extend(&mut destination, chunk);
        }
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, Default)]
pub enum SizeLimit {
    #[default]
    Unknown,
    Unlimited,
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
            Buf,
            BufMut,
        };

        #[test]
        fn write_with_fill() {
            let mut bytes_mut = Vec::<u8>::new();
            bytes_mut.write(4..8, b"abcd", ..).unwrap();
            assert_eq!(
                bytes_mut.chunks(..).unwrap().next().unwrap(),
                b"\x00\x00\x00\x00abcd"
            );
        }

        #[test]
        fn write_over_buf_end() {
            let mut bytes_mut = Vec::<u8>::new();
            bytes_mut.write(0..4, b"abcd", ..).unwrap();
            bytes_mut.write(2..6, b"efgh", ..).unwrap();
            assert_eq!(bytes_mut.chunks(..).unwrap().next().unwrap(), b"abefgh");
        }

        #[test]
        fn write_extend_with_unbounded_destination_slice() {
            let mut bytes_mut = Vec::<u8>::new();
            bytes_mut.write(0..4, b"abcd", ..).unwrap();
            bytes_mut.write(2.., b"efgh", ..).unwrap();
            assert_eq!(bytes_mut.chunks(..).unwrap().next().unwrap(), b"abefgh");
        }
    }
}
