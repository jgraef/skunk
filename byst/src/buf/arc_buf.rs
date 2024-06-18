use std::{
    cell::UnsafeCell,
    fmt::Debug,
    mem::MaybeUninit,
    ops::{
        Deref,
        DerefMut,
    },
    ptr::NonNull,
    sync::atomic::{
        AtomicUsize,
        Ordering,
    },
};

use super::{
    chunks::{
        SingleChunk,
        SingleChunkMut,
    },
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
    util::{
        buf_eq,
        debug_as_hexdump,
        ptr_len,
    },
    Buf,
    BufMut,
    IndexOutOfBounds,
    Range,
    RangeOutOfBounds,
};

#[derive(Clone, Copy)]
struct Buffer {
    /// Made from a `Box<[MaybeUninit<u8>]>>`
    ///
    /// This pointer is valid as long as a [`BufferOwned`] exists, or a
    /// [`BufferRef`] exists. therefore, as long as you can acquire this
    /// `BufferPtr`, it's safe to assume that `buf` points to valid
    /// memory.
    ///
    /// This may be dangling if the buffer is zero-sized. This means that no
    /// buffer was allocated for it, and thus must not be deallocated.
    buf: *const [UnsafeCell<MaybeUninit<u8>>],

    /// Made from a `Box<AtomicRefCount>`
    ///
    /// Invariant: This pointer is valid as long as [`BufferOwned`] exists, or
    /// a [`BufferRef`] exists.
    ///
    /// This may be `null` if the buffer is zero-sized. This means that no
    /// buffer was allocated for it, and thus must not be deallocated.
    ref_count: *const AtomicRefCount,
}

impl Buffer {
    fn zero_sized() -> Self {
        // special case for zero-sized buffers. they don't need to be reference counted,
        // and use a dangling pointer for the `buf`.

        let buf = unsafe {
            std::slice::from_raw_parts(
                NonNull::<UnsafeCell<MaybeUninit<u8>>>::dangling().as_ptr(),
                0,
            )
        };

        Self {
            buf,
            ref_count: std::ptr::null(),
        }
    }

    fn new(size: usize, ref_count: usize, owned: bool) -> Self {
        if size == 0 {
            Self::zero_sized()
        }
        else {
            // allocate ref_count
            let ref_count = Box::into_raw(Box::new(AtomicRefCount::new(ref_count, owned)));

            // allocate buffer
            let buf = Box::<[u8]>::new_uninit_slice(size);

            // leak it to raw pointer
            let buf = Box::into_raw(buf);

            // make it `*const [UnsafeCell<_>>]`.  This is roughly what
            // `UnsafeCell::from_mut` does.
            let buf = buf as *const [UnsafeCell<MaybeUninit<u8>>];

            Buffer { buf, ref_count }
        }
    }

    fn len(&self) -> usize {
        ptr_len(self.buf)
    }

    #[inline]
    unsafe fn deallocate(self) {
        assert!(
            !self.ref_count.is_null(),
            "Trying to deallocate a zero-sized Buffer"
        );
        let _ref_count = Box::from_raw(self.ref_count as *mut AtomicUsize);
        let _buf = Box::from_raw(self.buf as *mut [UnsafeCell<MaybeUninit<u8>>]);
    }

    #[inline]
    fn ref_count(&self) -> RefCount {
        if self.ref_count.is_null() {
            RefCount::Static
        }
        else {
            unsafe {
                // SAFETY: This `Buffer` only becomes invalid, if it's deallocated, but that
                // method is unsafe.
                RefCount::from_atomic(&*self.ref_count)
            }
        }
    }
}

/// This manages the reference count of a [`Buffer`]:
///
/// - [`Buffer`]s can have *one* reference from a [`Reclaim`]. This is stored as
///   the LSB.
/// - [`Buffer`]s can have any number of references through [`BufferRef`]. This
///   is stored in the remaining bits.
struct AtomicRefCount(AtomicUsize);

impl AtomicRefCount {
    #[inline]
    fn new(ref_count: usize, reclaim: bool) -> Self {
        Self(AtomicUsize::new(
            ref_count << 1 | if reclaim { 1 } else { 0 },
        ))
    }

    /// Increments reference count for [`BufferRef`]s
    #[inline]
    fn increment(&self) {
        self.0.fetch_add(2, Ordering::Relaxed);
    }

    /// Decrements reference count for [`BufferRef`]s and returns whether the
    /// buffer must be deallocated.
    #[inline]
    fn decrement(&self) -> MustDrop {
        let old_value = self.0.fetch_sub(2, Ordering::Relaxed);
        assert!(old_value >= 2);
        MustDrop(old_value == 2)
    }

    /// Removes the [`Reclaim`] reference and returns whether the buffer must be
    /// deallocated.
    #[inline]
    fn make_unreclaimable(&self) -> MustDrop {
        MustDrop(self.0.fetch_and(!1, Ordering::Relaxed) == 1)
    }

    /// Trys to reclaim the buffer. This will only be successful if the
    /// reclaim-reference is the only one to the buffer. In this case it'll
    /// increase the normal ref-count and return `true`.
    #[inline]
    fn try_reclaim(&self) -> bool {
        self.0
            .compare_exchange(1, 3, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }

    /// Checks if the buffer can be reclaimed.
    #[inline]
    fn can_reclaim(&self) -> bool {
        self.0.load(Ordering::Relaxed) == 1
    }
}

#[derive(Clone, Copy, Debug)]
#[must_use]
struct MustDrop(pub bool);

impl From<MustDrop> for bool {
    fn from(value: MustDrop) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug)]
pub enum RefCount {
    Static,
    Counted { ref_count: usize, reclaim: bool },
}

impl RefCount {
    fn from_atomic(value: &AtomicRefCount) -> Self {
        let value = value.0.load(Ordering::Relaxed);
        let ref_count = value >> 1;
        Self::Counted {
            ref_count,
            reclaim: value & 1 != 0,
        }
    }

    #[inline]
    pub fn ref_count(&self) -> Option<usize> {
        match self {
            Self::Static => None,
            Self::Counted { ref_count, .. } => Some(*ref_count),
        }
    }

    #[inline]
    pub fn can_be_reclaimed(&self) -> bool {
        match self {
            RefCount::Static => false,
            RefCount::Counted { reclaim, .. } => *reclaim,
        }
    }

    #[inline]
    pub fn is_static(&self) -> bool {
        matches!(self, Self::Static)
    }
}

struct BufferRef {
    buf: Buffer,
    // todo: replace `start` and `end` with a `*const [UnsafeCell<MaybeUninit<u8>>]` for that
    // portion of the buffer.
    start: usize,
    end: usize,
}

impl BufferRef {
    /// # Safety
    ///
    /// The caller must ensure that there are no mutable references to this
    /// portion of the buffer, and that the range is valid.
    #[inline]
    unsafe fn bytes<'a>(&'a self) -> &'a [MaybeUninit<u8>] {
        let ptr = self.buf.buf.get_unchecked(self.start..self.end);
        std::slice::from_raw_parts(UnsafeCell::raw_get(ptr.as_ptr()), self.len())
    }

    /// # Safety
    ///
    /// The caller must ensure that the access is unique, and that the range is
    /// valid. No other active references, mutable or not may exist to this port
    /// of the buffer.
    #[inline]
    unsafe fn bytes_mut<'a>(&'a self) -> &'a mut [MaybeUninit<u8>] {
        let ptr = self.buf.buf.get_unchecked(self.start..self.end);
        std::slice::from_raw_parts_mut(UnsafeCell::raw_get(ptr.as_ptr()), self.len())
    }

    #[inline]
    fn len(&self) -> usize {
        self.end - self.start
    }

    /// Splits `self` into:
    ///
    /// 1. `self`: `[at..]`
    /// 2. returns: `[..at)`
    fn split_at(&mut self, at: usize) -> BufferRef {
        let split_offset = at + self.start;
        assert!(split_offset <= self.end);
        let mut new = self.clone();
        self.start = split_offset;
        new.end = split_offset;
        new
    }

    fn shrink(&mut self, start: usize, end: usize) {
        let new_start = self.start + start;
        let new_end = self.start + end;
        assert!(new_start >= self.start);
        assert!(new_end <= self.end);
        self.start = new_start;
        self.end = new_end;
    }
}

impl Default for BufferRef {
    #[inline]
    fn default() -> Self {
        Self {
            buf: Buffer::zero_sized(),
            start: 0,
            end: 0,
        }
    }
}

impl Clone for BufferRef {
    fn clone(&self) -> Self {
        if !self.buf.ref_count.is_null() {
            unsafe {
                // SAFETY: This `Buffer` only becomes invalid, if it's deallocated, but that
                // method is unsafe.
                (*self.buf.ref_count).increment();
            }
        }

        Self {
            buf: self.buf,
            start: self.start,
            end: self.end,
        }
    }
}

impl Drop for BufferRef {
    fn drop(&mut self) {
        if !self.buf.ref_count.is_null() {
            unsafe {
                // SAFETY: This drops the inner buffer, if the ref_count reaches 0. But we're
                // dropping our ref, so it's fine.
                if (*self.buf.ref_count).decrement().into() {
                    self.buf.deallocate();
                }
            }
        }
    }
}

pub struct Reclaim {
    buf: Buffer,
}

impl Reclaim {
    pub fn try_reclaim(&self) -> Option<ArcBufMut> {
        if self.buf.ref_count.is_null() {
            Some(ArcBufMut::default())
        }
        else {
            let reclaimed = unsafe {
                // SAFETY: We have a [`Reclaim`] reference to the buffer, so it hasn't been
                // deallocated. Thus it's safe to dereference the `ref_count`.
                (*self.buf.ref_count).try_reclaim()
            };

            reclaimed.then(|| {
                // we reclaimed the buffer, thus we can hand out a new reference to it :)
                ArcBufMut {
                    inner: BufferRef {
                        buf: self.buf,
                        start: 0,
                        end: self.buf.len(),
                    },
                    initialized: 0,
                }
            })
        }
    }

    #[inline]
    pub fn can_reclaim(&self) -> bool {
        if !self.buf.ref_count.is_null() {
            unsafe {
                // SAFETY: We have a [`Reclaim`] reference to the buffer, so it hasn't been
                // deallocated. Thus it's safe to dereference the `ref_count`.
                (*self.buf.ref_count).can_reclaim()
            }
        }
        else {
            true
        }
    }

    #[inline]
    pub fn ref_count(&self) -> RefCount {
        self.buf.ref_count()
    }
}

impl Drop for Reclaim {
    fn drop(&mut self) {
        if !self.buf.ref_count.is_null() {
            unsafe {
                // SAFETY: We have a [`Reclaim`] reference to the buffer, so it hasn't been
                // deallocated. Thus it's safe to dereference the `ref_count`.
                if (*self.buf.ref_count).make_unreclaimable().into() {
                    self.buf.deallocate();
                }
            }
        }
    }
}

impl Debug for Reclaim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reclaim").finish_non_exhaustive()
    }
}

#[derive(Clone, Default)]
pub struct ArcBuf {
    inner: BufferRef,
}

impl ArcBuf {
    #[inline]
    fn bytes(&self) -> &[u8] {
        unsafe { MaybeUninit::slice_assume_init_ref(self.inner.bytes()) }
    }

    #[inline]
    pub fn ref_count(&self) -> RefCount {
        self.inner.buf.ref_count()
    }
}

impl Buf for ArcBuf {
    type View<'a> = Self
    where
        Self: 'a;

    type Chunks<'a> = SingleChunk<'a>
    where
        Self: 'a;

    fn view(&self, range: impl Into<Range>) -> Result<Self::View<'_>, RangeOutOfBounds> {
        let (start, end) = range.into().indices_checked_in(0, self.inner.len())?;
        if start == end {
            Ok(Self::default())
        }
        else {
            let mut inner = self.inner.clone();
            inner.shrink(start, end);
            Ok(ArcBuf { inner })
        }
    }

    fn chunks(&self, range: impl Into<Range>) -> Result<Self::Chunks<'_>, RangeOutOfBounds> {
        Ok(SingleChunk::new(range.into().slice_get(self.bytes())?))
    }
}

impl BytesImpl for ArcBuf {
    fn view(&self, range: Range) -> Result<Box<dyn BytesImpl>, RangeOutOfBounds> {
        Ok(Box::new(Buf::view(self, range)?))
    }

    fn chunks(&self, range: Range) -> Result<Box<dyn ChunksIterImpl<'_> + '_>, RangeOutOfBounds> {
        Ok(Box::new(Buf::chunks(self, range)?))
    }

    fn clone(&self) -> Box<dyn BytesImpl> {
        Box::new(Clone::clone(self))
    }
}

impl Length for ArcBuf {
    #[inline]
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl Deref for ArcBuf {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.bytes()
    }
}

impl AsRef<[u8]> for ArcBuf {
    fn as_ref(&self) -> &[u8] {
        self.bytes()
    }
}

impl Debug for ArcBuf {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        debug_as_hexdump(f, self.bytes())
    }
}

impl<T: Buf> PartialEq<T> for ArcBuf {
    #[inline]
    fn eq(&self, other: &T) -> bool {
        buf_eq(self, other)
    }
}

#[derive(Default)]
pub struct ArcBufMut {
    inner: BufferRef,
    initialized: usize,
}

impl ArcBufMut {
    pub fn new(capacity: usize) -> Self {
        let buf = Buffer::new(capacity, 1, false);
        Self {
            inner: BufferRef {
                buf,
                start: 0,
                end: buf.len(),
            },
            initialized: 0,
        }
    }

    pub fn new_reclaimable(capacity: usize) -> (Self, Reclaim) {
        let buf = Buffer::new(capacity, 1, true);
        let this = Self {
            inner: BufferRef {
                buf,
                start: 0,
                end: buf.len(),
            },
            initialized: 0,
        };
        let reclaim = Reclaim { buf };
        (this, reclaim)
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.inner.len()
    }

    pub fn copy_from_slice(from: &[u8]) -> Self {
        let mut this = Self::new(from.len());
        BufMut::extend(&mut this, from).unwrap();
        this
    }

    pub fn freeze(mut self) -> ArcBuf {
        if self.initialized == 0 {
            ArcBuf::default()
        }
        else {
            self.inner.shrink(0, self.initialized);
            ArcBuf { inner: self.inner }
        }
    }

    #[inline]
    pub fn ref_count(&self) -> RefCount {
        self.inner.buf.ref_count()
    }

    /// Splits `self` into:
    ///
    /// 1. `self`: `[at..]`
    /// 2. returns: `[..at)`
    pub fn split_at(&mut self, at: usize) -> Result<ArcBufMut, IndexOutOfBounds> {
        let initialized = self.initialized;
        if at == 0 {
            Ok(Self::default())
        }
        else if at == initialized {
            Ok(std::mem::replace(self, Self::default()))
        }
        else if at < initialized {
            let inner = self.inner.split_at(at);
            self.initialized = at;
            Ok(Self {
                inner,
                initialized: initialized - at,
            })
        }
        else {
            Err(IndexOutOfBounds {
                required: at,
                bounds: (0, initialized),
            })
        }
    }

    #[inline]
    fn bytes(&self) -> &[u8] {
        unsafe { MaybeUninit::slice_assume_init_ref(&self.inner.bytes()[..self.initialized]) }
    }

    #[inline]
    fn bytes_mut(&self) -> &mut [u8] {
        unsafe {
            MaybeUninit::slice_assume_init_mut(&mut self.inner.bytes_mut()[..self.initialized])
        }
    }
}

impl Deref for ArcBufMut {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.bytes()
    }
}

impl DerefMut for ArcBufMut {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.bytes_mut()
    }
}

impl AsRef<[u8]> for ArcBufMut {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.bytes()
    }
}

impl AsMut<[u8]> for ArcBufMut {
    #[inline]
    fn as_mut(&mut self) -> &mut [u8] {
        self.bytes_mut()
    }
}

impl Debug for ArcBufMut {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        debug_as_hexdump(f, self.bytes())
    }
}

impl<T: Buf> PartialEq<T> for ArcBufMut {
    #[inline]
    fn eq(&self, other: &T) -> bool {
        buf_eq(self, other)
    }
}

impl Buf for ArcBufMut {
    type View<'a> = &'a [u8]
    where
        Self: 'a;

    type Chunks<'a> = SingleChunk<'a>
    where
        Self: 'a;

    fn view(&self, range: impl Into<Range>) -> Result<Self::View<'_>, RangeOutOfBounds> {
        Ok(range.into().slice_get(self.bytes())?)
    }

    fn chunks(&self, range: impl Into<Range>) -> Result<Self::Chunks<'_>, RangeOutOfBounds> {
        Ok(SingleChunk::new(range.into().slice_get(self.bytes())?))
    }
}

impl Length for ArcBufMut {
    #[inline]
    fn len(&self) -> usize {
        self.initialized
    }
}

impl BufMut for ArcBufMut {
    type ViewMut<'a> = &'a mut [u8]
    where
        Self: 'a;

    type ChunksMut<'a> = SingleChunkMut<'a>
    where
        Self: 'a;

    fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds> {
        Ok(range.into().slice_get_mut(self.bytes_mut())?)
    }

    fn chunks_mut(
        &mut self,
        range: impl Into<Range>,
    ) -> Result<Self::ChunksMut<'_>, RangeOutOfBounds> {
        Ok(SingleChunkMut::new(
            range.into().slice_get_mut(self.bytes_mut())?,
        ))
    }

    fn reserve(&mut self, size: usize) -> Result<(), Full> {
        if size <= self.capacity() {
            Ok(())
        }
        else {
            Err(Full {
                required: size,
                capacity: self.capacity(),
            })
        }
    }

    fn grow(&mut self, new_len: usize, value: u8) -> Result<(), super::Full> {
        if new_len <= self.capacity() {
            if new_len > self.initialized {
                unsafe {
                    MaybeUninit::fill(
                        &mut self.inner.bytes_mut()[self.initialized..new_len],
                        value,
                    );
                    self.initialized = new_len;
                }
            }
            Ok(())
        }
        else {
            Err(Full {
                required: new_len,
                capacity: self.capacity(),
            })
        }
    }

    fn extend(&mut self, with: &[u8]) -> Result<(), super::Full> {
        let new_len = self.initialized + with.len();
        if new_len <= self.capacity() {
            if new_len > self.initialized {
                unsafe {
                    MaybeUninit::fill_from(
                        &mut self.inner.bytes_mut()[self.initialized..new_len],
                        with.iter().copied(),
                    );
                    self.initialized = new_len;
                }
            }
            Ok(())
        }
        else {
            Err(Full {
                required: new_len,
                capacity: self.capacity(),
            })
        }
    }

    #[inline]
    fn size_limit(&self) -> SizeLimit {
        SizeLimit::Exact(self.capacity())
    }
}

impl BytesMutImpl for ArcBufMut {
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
        mut self,
        at: usize,
    ) -> Result<(Box<dyn BytesMutImpl>, Box<dyn BytesMutImpl>), IndexOutOfBounds> {
        // todo: sucks to re-allocate a `Box` for self here :/
        let other = ArcBufMut::split_at(&mut self, at)?;
        Ok((Box::new(self), Box::new(other)))
    }
}

#[cfg(test)]
mod tests {
    // most tests are in `crate::slab`, but tests here would be nice too :3

    use super::ArcBufMut;

    #[test]
    fn it_reclaims_empty_buffers_correctly() {
        // don't ask me why we have specifically this test lol
        let (buf, reclaim) = ArcBufMut::new_reclaimable(0);
        assert!(buf.inner.buf.ref_count.is_null());
        assert!(buf.ref_count().is_static());
        drop(buf);
        assert!(reclaim.can_reclaim());
        let reclaimed = reclaim.try_reclaim().unwrap();
        assert!(reclaimed.ref_count().is_static());
    }

    #[test]
    fn empty_bufs_dont_ref_count() {
        let buf = ArcBufMut::new(10);
        let frozen = buf.freeze();
        assert_eq!(frozen.ref_count().ref_count(), None);
    }
}
