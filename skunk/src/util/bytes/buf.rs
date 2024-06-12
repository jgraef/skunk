use std::{
    borrow::Cow,
    fmt::Debug,
    iter::{
        Flatten,
        FusedIterator,
    },
    sync::Arc,
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
                type View<'a> = & $view_lt [u8] where Self: 'a;

                type Chunks<'a> = SingleChunk<'a> where Self: 'a;

                #[inline]
                fn view(&self, range: impl Into<Range>) -> Result<Self::View<'_>, RangeOutOfBounds> {
                    range.into().slice_get(self)
                }

                #[inline]
                fn chunks(&self, range: impl Into<Range>) -> Result<Self::Chunks<'_>, RangeOutOfBounds> {
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
    let destination_start = destination_range.start().unwrap_or_default();
    let (destination_end, destination_range_length) =
        if let Some(destination_end) = destination_range.end() {
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

/// Chunk iterator for contiguous buffers.
#[derive(Clone, Copy, Debug)]
pub struct SingleChunk<'a> {
    chunk: &'a [u8],
    exhausted: bool,
}

impl<'a> SingleChunk<'a> {
    #[inline]
    pub fn new(chunk: &'a [u8]) -> Self {
        Self {
            chunk,
            exhausted: chunk.is_empty(),
        }
    }

    #[inline]
    pub fn get(&self) -> &'a [u8] {
        self.chunk
    }
}

impl<'a> Iterator for SingleChunk<'a> {
    type Item = &'a [u8];

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.exhausted {
            None
        }
        else {
            self.exhausted = true;
            Some(self.chunk)
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.exhausted.then_some(0).unwrap_or(1);
        (n, Some(n))
    }
}

impl<'a> DoubleEndedIterator for SingleChunk<'a> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.next()
    }
}

impl<'a> ExactSizeIterator for SingleChunk<'a> {}

impl<'a> FusedIterator for SingleChunk<'a> {}

/// Mutable chunk iterator for contiguous buffers.
#[derive(Debug)]
pub struct SingleChunkMut<'a> {
    chunk: Option<&'a mut [u8]>,
}

impl<'a> SingleChunkMut<'a> {
    #[inline]
    pub fn new(chunk: &'a mut [u8]) -> Self {
        Self {
            chunk: (!chunk.is_empty()).then_some(chunk),
        }
    }
}

impl<'a> Iterator for SingleChunkMut<'a> {
    type Item = &'a mut [u8];

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.chunk.take()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.chunk.is_some().then_some(1).unwrap_or_default();
        (n, Some(n))
    }
}

impl<'a> DoubleEndedIterator for SingleChunkMut<'a> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.chunk.take()
    }
}

impl<'a> ExactSizeIterator for SingleChunkMut<'a> {}

impl<'a> FusedIterator for SingleChunkMut<'a> {}

/// Iterator over the bytes in a buffer.
#[derive(Debug)]
pub struct BufIter<'b, B: Buf + ?Sized + 'b> {
    inner: Flatten<B::Chunks<'b>>,
}

impl<'b, B: Buf + ?Sized> BufIter<'b, B> {
    #[inline]
    fn new(chunks: B::Chunks<'b>) -> Self {
        Self {
            inner: chunks.flatten(),
        }
    }
}

impl<'b, B: Buf + ?Sized> Iterator for BufIter<'b, B> {
    type Item = u8;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().copied()
    }
}

impl<'b, B: Buf + ?Sized> DoubleEndedIterator for BufIter<'b, B>
where
    <B as Buf>::Chunks<'b>: DoubleEndedIterator,
{
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back().copied()
    }
}

impl<'b, B: Buf + ?Sized> FusedIterator for BufIter<'b, B> {}

/// Mutable iterator over the bytes in a buffer.
pub struct BufIterMut<'b, B: BufMut + ?Sized + 'b> {
    inner: Flatten<B::ChunksMut<'b>>,
}

impl<'b, B: BufMut + ?Sized> BufIterMut<'b, B> {
    #[inline]
    fn new(chunks: B::ChunksMut<'b>) -> Self {
        Self {
            inner: chunks.flatten(),
        }
    }
}

impl<'b, B: BufMut + ?Sized> Iterator for BufIterMut<'b, B> {
    type Item = &'b mut u8;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<'b, B: BufMut + ?Sized> DoubleEndedIterator for BufIterMut<'b, B>
where
    <B as BufMut>::ChunksMut<'b>: DoubleEndedIterator,
{
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back()
    }
}

impl<'b, B: BufMut + ?Sized> FusedIterator for BufIterMut<'b, B> {}

/// Iterator wrapper to skip empty chunks.
#[derive(Debug)]
pub struct NonEmptyIter<I>(pub I);

impl<T: AsRef<[u8]>, I: Iterator<Item = T>> Iterator for NonEmptyIter<I> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let x = self.0.next()?;
        (!x.as_ref().is_empty()).then_some(x)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, self.0.size_hint().1)
    }
}

impl<T: AsRef<[u8]>, I: Iterator<Item = T> + DoubleEndedIterator> DoubleEndedIterator
    for NonEmptyIter<I>
{
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back()
    }
}

impl<T: AsRef<[u8]>, I: Iterator<Item = T>> FusedIterator for NonEmptyIter<I> {}

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
