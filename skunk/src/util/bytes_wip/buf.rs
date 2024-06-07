use std::{
    borrow::Cow,
    fmt::Debug,
    iter::{
        Flatten,
        FusedIterator,
    },
    ops::{
        Bound,
        RangeBounds,
        RangeFull,
    },
    sync::Arc,
};

use super::{
    read::BufReader,
    slice_get_mut_range,
    slice_get_range,
};

#[derive(Debug, thiserror::Error)]
#[error("Range out of bounds: {:?} not in buffer (..{buf_length})", DebugRange(.range))]
pub struct RangeOutOfBounds<R: RangeBounds<usize>> {
    pub range: R,
    pub buf_length: usize,
}

struct DebugRange<'r, R: RangeBounds<usize>>(&'r R);

impl<'r, R: RangeBounds<usize>> Debug for DebugRange<'r, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0.start_bound() {
            Bound::Included(start) => write!(f, "{start}")?,
            Bound::Excluded(start) => write!(f, "{}", *start + 1)?,
            Bound::Unbounded => {}
        }
        write!(f, "..")?;
        match self.0.end_bound() {
            Bound::Included(end) => write!(f, "{end}")?,
            Bound::Excluded(end) => write!(f, "={end}")?,
            Bound::Unbounded => {}
        }
        Ok(())
    }
}

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
    fn view<R: RangeBounds<usize>>(&self, range: R) -> Result<Self::View<'_>, RangeOutOfBounds<R>>;

    /// Returns an iterator over contiguous byte chunks that make up this
    /// buffer.
    fn chunks<R: RangeBounds<usize>>(
        &self,
        range: R,
    ) -> Result<Self::Chunks<'_>, RangeOutOfBounds<R>>;

    /// Returns the length of this buffer in bytes.
    fn len(&self) -> usize;

    /// Returns whether this buffer is empty (i.e. has length 0).
    #[inline]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    fn reader(self) -> BufReader<Self>
    where
        Self: Sized,
    {
        BufReader::new(self)
    }

    #[inline]
    fn iter<R: RangeBounds<usize>>(
        &self,
        range: R,
    ) -> Result<BufIter<'_, Self>, RangeOutOfBounds<R>> {
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
                fn view<R: RangeBounds<usize>>(&self, range: R) -> Result<Self::View<'_>, RangeOutOfBounds<R>> {
                    <B as Buf>::view(*self, range)
                }

                #[inline]
                fn chunks<R: RangeBounds<usize>>(&self, range: R) -> Result<Self::Chunks<'_>, RangeOutOfBounds<R>> {
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
    ('b, B: Buf), &'b B;
    ('b, B: Buf), &'b mut B;
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
                fn view<R: RangeBounds<usize>>(&self, range: R) -> Result<Self::View<'_>, RangeOutOfBounds<R>> {
                    slice_get_range(self, range)
                }

                #[inline]
                fn chunks<R: RangeBounds<usize>>(&self, range: R) -> Result<Self::Chunks<'_>, RangeOutOfBounds<R>> {
                    Ok(SingleChunk::new(slice_get_range(self, range)?))
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
    fn view_mut<R: RangeBounds<usize>>(
        &mut self,
        range: R,
    ) -> Result<Self::ViewMut<'_>, RangeOutOfBounds<R>>;

    /// Returns an iterator over contiguous mutable byte chunks that make up
    /// this buffer.
    fn chunks_mut<R: RangeBounds<usize>>(
        &mut self,
        range: R,
    ) -> Result<Self::ChunksMut<'_>, RangeOutOfBounds<R>>;

    #[inline]
    fn iter_mut<R: RangeBounds<usize>>(
        &mut self,
        range: R,
    ) -> Result<BufIterMut<'_, Self>, RangeOutOfBounds<R>> {
        Ok(BufIterMut::new(self.chunks_mut(range)?))
    }
}

impl<'b, B: BufMut> BufMut for &'b mut B {
    type ViewMut<'a> = <B as BufMut>::ViewMut<'a> where Self: 'a;

    type ChunksMut<'a> = <B as BufMut>::ChunksMut<'a> where Self: 'a;

    #[inline]
    fn view_mut<R: RangeBounds<usize>>(
        &mut self,
        range: R,
    ) -> Result<Self::ViewMut<'_>, RangeOutOfBounds<R>> {
        <B as BufMut>::view_mut(*self, range)
    }

    #[inline]
    fn chunks_mut<R: RangeBounds<usize>>(
        &mut self,
        range: R,
    ) -> Result<Self::ChunksMut<'_>, RangeOutOfBounds<R>> {
        <B as BufMut>::chunks_mut(*self, range)
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
                fn view_mut<R: RangeBounds<usize>>(&mut self, range: R) -> Result<Self::ViewMut<'_>, RangeOutOfBounds<R>> {
                    slice_get_mut_range(self, range)
                }

                #[inline]
                fn chunks_mut<R: RangeBounds<usize>>(
                    &mut self,
                    range: R,
                ) -> Result<Self::ChunksMut<'_>, RangeOutOfBounds<R>>
                {
                    Ok(SingleChunkMut::new(slice_get_mut_range(self, range)?))
                }
            }
        )*
    };
}

impl_buf_mut_for_slice_like! {
    ('b), &'b mut [u8];
    (const N: usize), [u8; N];
    (), Vec<u8>;
    (), Box<[u8]>;
}

/// Error while copying from a [`Buf`] to a [`BufMut`].
#[derive(Debug, thiserror::Error)]
pub enum CopyError<D: RangeBounds<usize>, S: RangeBounds<usize>> {
    #[error("Destination index out of bounds")]
    DestinationRangeOutOfBounds(RangeOutOfBounds<D>),

    #[error("Source index out of bounds")]
    SourceRangeOutOfBounds(RangeOutOfBounds<S>),

    #[error("The destination buffer has remaining space after exhausting the source buffer")]
    DestinationSpaceRemaining,

    #[error("The source buffer has remaining data after filling the destination buffer")]
    SourceDataRemaining,
}

/// Copies bytes from `source` to `destination` with respective ranges.
///
/// This can fail if either range is out of bounds, or the lengths of both
/// ranges doesn't match up. See [`CopyError`].
pub fn copy<D: RangeBounds<usize>, S: RangeBounds<usize>>(
    mut destination: impl BufMut,
    destination_range: D,
    source: impl Buf,
    source_range: S,
) -> Result<(), CopyError<D, S>> {
    // note: we can't actually check if the lengths match up here, because either
    // range can be open (e.g. `123..`).

    let mut dest_chunks = destination
        .chunks_mut(destination_range)
        .map_err(CopyError::DestinationRangeOutOfBounds)?;
    let mut src_chunks = source
        .chunks(source_range)
        .map_err(CopyError::SourceRangeOutOfBounds)?;

    let mut current_dest_chunk: Option<&mut [u8]> = dest_chunks.next();
    let mut current_src_chunk: Option<&[u8]> = src_chunks.next();

    let mut dest_pos = 0;
    let mut src_pos = 0;

    loop {
        match (&mut current_dest_chunk, current_src_chunk) {
            (None, None) => break Ok(()),
            (Some(dest_chunk), Some(src_chunk)) => {
                let n = std::cmp::min(dest_chunk.len() - dest_pos, src_chunk.len() - src_pos);

                dest_chunk[dest_pos..][..n].copy_from_slice(&src_chunk[src_pos..][..n]);

                dest_pos += n;
                src_pos += n;

                if dest_pos == dest_chunk.len() {
                    current_dest_chunk = dest_chunks.next();
                    dest_pos = 0;
                }
                if src_pos == src_chunk.len() {
                    current_src_chunk = src_chunks.next();
                    dest_pos = 0;
                }
            }
            (Some(dest_chunk), None) => {
                if dest_chunk.is_empty() {
                    current_dest_chunk = dest_chunks.next();
                }
                else {
                    break Err(CopyError::DestinationSpaceRemaining);
                }
            }
            (None, Some(src_chunk)) => {
                if src_chunk.is_empty() {
                    current_src_chunk = src_chunks.next();
                }
                else {
                    break Err(CopyError::SourceDataRemaining);
                }
            }
        }
    }
}

/// Chunk iterator for contiguous buffers.
#[derive(Debug)]
pub struct SingleChunk<'a> {
    chunk: Option<&'a [u8]>,
}

impl<'a> SingleChunk<'a> {
    #[inline]
    pub fn new(chunk: &'a [u8]) -> Self {
        Self { chunk: Some(chunk) }
    }
}

impl<'a> Iterator for SingleChunk<'a> {
    type Item = &'a [u8];

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

impl<'a> DoubleEndedIterator for SingleChunk<'a> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.chunk.take()
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
        Self { chunk: Some(chunk) }
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
    type Item = u8;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().copied()
    }
}

impl<'b, B: BufMut + ?Sized> FusedIterator for BufIterMut<'b, B> {}

// todo: buffers than can grow (and shrink?). this should be implemented for
// `ArrayBuf`, `Vec` and `VecDeque`. for `VecDeque` we have the interesting case
// where it could also shrink from the beginning. but then we would need to make
// sure that indices stay consistent, e.g. by tracking the apparent start index
// of the buffer. we would probably just want to implement our own `RingBuf`. we
// would also need a trait to shrink/grow from the start of the buffer.
pub trait Grow: BufMut {}
