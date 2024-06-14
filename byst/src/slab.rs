use std::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    ptr::NonNull,
    sync::atomic::{
        AtomicUsize,
        Ordering,
    },
};

use super::{
    buf::{
        chunks::{
            SingleChunk,
            SingleChunkMut,
        },
        write_helper,
        WriteError,
    },
    Buf,
    BufMut,
    Range,
    RangeOutOfBounds,
};
use crate::util::ptr_len;

/// Efficient allocation of equally-sized buffers.
pub struct Slab {
    buf_size: usize,
    reuse_count: usize,
    in_use: Vec<BufferOwned>,
    available: Vec<BufferOwned>,
}

impl Slab {
    /// Creates a new slab allocator for buffers of size `buf_size`.
    ///
    /// The argument `reuse_count` controls how many buffers should be kept
    /// around for reuse. If a buffer becomes available again, but the [`Slab`]
    /// already has `reuse_count` available buffers, it will free the
    /// newly-available buffer.
    #[inline]
    pub fn new(buf_size: usize, reuse_count: usize) -> Self {
        Self {
            buf_size,
            reuse_count,
            in_use: Vec::with_capacity(reuse_count * 2),
            available: Vec::with_capacity(reuse_count),
        }
    }

    /// Returns a (mutable) buffer.
    ///
    /// This will either reuse a buffer, or allocate a new one.
    ///
    /// Once the returned [`BytesMut`] or all [`Bytes`] created from it are
    /// dropped, the buffer will be reused by the [`Slab`].
    pub fn get(&mut self) -> BytesMut {
        if self.buf_size == 0 {
            return BytesMut::default();
        }

        let buf = if let Some(buf) = self.available.pop() {
            // there's a buffer in `available` that we can use.
            let buf_ref = buf.get_ref();
            self.in_use.push(buf);
            buf_ref
        }
        else {
            // try to find a buffer that is unused, but not yet in `available`.

            let mut i = 0;
            let mut reclaimed = None;

            while i < self.in_use.len() {
                let buf = &self.in_use[i];

                if reclaimed.is_none() {
                    // try to reclaim unused buffer

                    if buf.try_reclaim() {
                        reclaimed = Some(buf.get_ref());
                    }
                    else {
                        i += 1;
                    }
                }
                else {
                    // get all other buffers with `ref_count=1` from `in_use` and put them into
                    // `available`, or drop them.

                    if buf.is_reclaimable() {
                        let buf = self.in_use.swap_remove(i);
                        if self.available.len() < self.reuse_count {
                            // put buffer into available list
                            self.available.push(buf);
                        }
                    }
                    else {
                        i += 1;
                    }
                }
            }

            if let Some(reclaimed) = reclaimed {
                reclaimed
            }
            else {
                // allocate new buffer
                let (buf, buf_ref) = BufferOwned::new(self.buf_size);
                self.in_use.push(buf);
                buf_ref
            }
        };

        BytesMut::new(buf)
    }

    /// Number of in-use buffers.
    ///
    /// This value might not be accurate, because the [`Slab`] only checks if
    /// buffers became available again, if none are available during
    /// [`Slab::get`]. Thus the actual number of in-use buffers might be lower.
    #[inline]
    pub fn num_in_use(&self) -> usize {
        self.in_use.len()
    }

    /// Number of available buffers.
    ///
    /// This value might not be accurate, because the [`Slab`] only checks if
    /// buffers became available again, if none are available during
    /// [`Slab::get`]. Thus the actual number of available buffers might be
    /// higher.
    #[inline]
    pub fn num_available(&self) -> usize {
        self.available.len()
    }

    /// Total number of buffers managed by this [`Slab`]
    #[inline]
    pub fn num_total(&self) -> usize {
        self.num_in_use() + self.num_available()
    }

    /// Size of buffer this [`Slab`] allocates
    #[inline]
    pub fn buf_size(&self) -> usize {
        self.buf_size
    }

    /// The `reuse_count` with which this [`Slab`] was configured. See
    /// [`Slab::new`].
    #[inline]
    pub fn reuse_count(&self) -> usize {
        self.reuse_count
    }

    /// Change the `reuse_cunt`. See [`Slab::new`].
    #[inline]
    pub fn set_reuse_count(&mut self, reuse_count: usize) {
        self.reuse_count = reuse_count;
    }
}

/// A mutable buffer.
///
/// This implements [`Buf`] and [`BufMut`], which provide most of its
/// functionality.
///
/// Furthermore the [`freeze`](Self::freeze) method can be used to turn this
/// into a [`Bytes`], which then can be shared.
#[derive(Default)]
pub struct BytesMut {
    inner: BytesInner,
}

impl BytesMut {
    #[inline]
    fn new(buf: BufferRef) -> Self {
        Self {
            inner: BytesInner {
                buf,
                offset: 0,
                len: 0,
            },
        }
    }

    #[inline]
    fn bytes_mut<'a>(&'a mut self) -> &'a mut [u8] {
        unsafe {
            // SAFETY:
            // There are not mutable reference to the buffer.
            self.inner.bytes_mut()
        }
    }

    fn resize(&mut self, new_len: usize, value: u8) {
        if new_len > self.buf_size() {
            panic!(
                "Can't resize BytesMut with buf_size={} to length {new_len}",
                self.buf_size()
            );
        }

        if new_len > self.inner.len {
            unsafe {
                MaybeUninit::fill(
                    &mut self.inner.buf.bytes_mut(self.inner.len..new_len),
                    value,
                );
            }
        }
        self.inner.len = new_len;
    }

    /// Turns this mutable buffer in a read-only sharable buffer.
    #[inline]
    pub fn freeze(self) -> Bytes {
        if self.inner.len == 0 {
            // if the `BytesMut` has length 0, we'll hand out a static buffer instead.
            Bytes::default()
        }
        else {
            Bytes { inner: self.inner }
        }
    }

    /// The size of the underlying buffer, i.e. how much this buffer can grow.
    #[inline]
    pub fn buf_size(&self) -> usize {
        self.inner.buf.len()
    }

    #[inline]
    pub fn ref_count(&self) -> RefCount {
        self.inner.ref_count()
    }
}

impl Buf for BytesMut {
    type View<'a> = &'a [u8]
    where
        Self: 'a;

    type Chunks<'a> = SingleChunk<'a>
    where
        Self: 'a;

    #[inline]
    fn view(&self, range: impl Into<Range>) -> Result<Self::View<'_>, RangeOutOfBounds> {
        range.into().slice_get(self.inner.bytes())
    }

    #[inline]
    fn chunks(&self, range: impl Into<Range>) -> Result<Self::Chunks<'_>, RangeOutOfBounds> {
        Ok(SingleChunk::new(
            range.into().slice_get(self.inner.bytes())?,
        ))
    }

    #[inline]
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl BufMut for BytesMut {
    type ViewMut<'a> = &'a mut [u8]
    where
        Self: 'a;

    type ChunksMut<'a> = SingleChunkMut<'a>
    where
        Self: 'a;

    #[inline]
    fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds> {
        range.into().slice_get_mut(self.bytes_mut())
    }

    #[inline]
    fn chunks_mut(
        &mut self,
        range: impl Into<Range>,
    ) -> Result<Self::ChunksMut<'_>, RangeOutOfBounds> {
        Ok(SingleChunkMut::new(
            range.into().slice_get_mut(self.bytes_mut())?,
        ))
    }

    fn grow_for(&mut self, range: impl Into<Range>) -> Result<(), RangeOutOfBounds> {
        let range = range.into();
        let new_len = range.len_in(0, self.inner.len);
        if new_len <= self.buf_size() {
            self.resize(new_len, 0);
            Ok(())
        }
        else {
            Err(RangeOutOfBounds {
                required: range,
                bounds: (0, self.buf_size()),
            })
        }
    }

    #[inline]
    fn write(
        &mut self,
        destination_range: impl Into<Range>,
        source: impl Buf,
        source_range: impl Into<Range>,
    ) -> Result<(), WriteError> {
        let buf_size = self.buf_size();
        write_helper(
            self,
            destination_range,
            &source,
            source_range,
            |_this, n| (n <= buf_size).then_some(()).ok_or(buf_size),
            |_, _| (),
            |this, n| this.resize(n, 0),
            |this, chunk| {
                unsafe {
                    MaybeUninit::copy_from_slice(
                        &mut this
                            .inner
                            .buf
                            .bytes_mut(this.inner.len..(this.inner.len + chunk.len())),
                        chunk,
                    );
                }
                this.inner.len += chunk.len();
            },
        )
    }
}

/// A share-able read-only buffer.
///
/// These are cheap to clone. Once all the [`Bytes`] refering to the underlying
/// buffer have been dropped, the buffer will be reused by the [`Slab`].
#[derive(Clone, Default)]
pub struct Bytes {
    inner: BytesInner,
}

impl Bytes {
    #[inline]
    fn bytes<'a>(&'a self) -> &'a [u8] {
        self.inner.bytes()
    }

    /// Try to make this into a [`BytesMut`].
    ///
    /// This checks if there are any other references to the underlying buffer,
    /// and promotes this [`Bytes`] into a [`BytesMut`], if possible.
    pub fn into_mut(self) -> Result<BytesMut, Self> {
        if self.inner.buf.is_exclusive() {
            Ok(BytesMut { inner: self.inner })
        }
        else {
            Err(self)
        }
    }

    #[inline]
    pub fn ref_count(&self) -> RefCount {
        self.inner.ref_count()
    }

    /// If `subset` is a slice contained in the [`Bytes`], this returns a view
    /// for that slice.
    ///
    /// This is useful if you're using some function that only returns a
    /// sub-slice `&[u8]` from a [`Bytes`], but you want to have that sub-slice
    /// as a view.
    #[inline]
    pub fn view_from_slice(&self, subset: &[u8]) -> Option<Self> {
        Some(Self {
            inner: self.inner.view_from_slice(subset)?,
        })
    }
}

impl Buf for Bytes {
    type View<'a> = Bytes
    where
        Self: 'a;

    type Chunks<'a> = SingleChunk<'a>
    where
        Self: 'a;

    fn view(&self, range: impl Into<Range>) -> Result<Self::View<'_>, RangeOutOfBounds> {
        let (start, end) = range
            .into()
            .indices_checked_in(self.inner.offset, self.inner.offset + self.inner.len)?;

        if start == end {
            Ok(Self {
                inner: BytesInner::default(),
            })
        }
        else {
            Ok(Self {
                inner: BytesInner {
                    buf: self.inner.buf.clone(),
                    offset: start,
                    len: end - start,
                },
            })
        }
    }

    fn chunks(&self, range: impl Into<Range>) -> Result<Self::Chunks<'_>, RangeOutOfBounds> {
        let (start, end) = range
            .into()
            .indices_checked_in(self.inner.offset, self.inner.offset + self.inner.len)?;
        Ok(SingleChunk::new(&self.bytes()[start..end]))
    }

    #[inline]
    fn len(&self) -> usize {
        self.inner.len
    }
}

#[derive(Clone, Default)]
struct BytesInner {
    buf: BufferRef,
    offset: usize,
    /// `0..len` is the initialized portion of the buffer
    len: usize,
}

impl BytesInner {
    #[inline]
    fn bytes<'a>(&'a self) -> &'a [u8] {
        unsafe {
            // SAFETY:
            // There are not mutable reference to the buffer. Technically `Slab`
            // also might have a reference to it, but never uses it to access the buffer.
            MaybeUninit::slice_assume_init_ref(
                self.buf.bytes(self.offset..(self.offset + self.len)),
            )
        }
    }

    /// SAFETY:
    /// The caller must ensure that there are no other references to this
    /// buffer.
    #[inline]
    unsafe fn bytes_mut<'a>(&'a mut self) -> &'a mut [u8] {
        MaybeUninit::slice_assume_init_mut(
            self.buf.bytes_mut(self.offset..(self.offset + self.len)),
        )
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// If `subset` is a slice contained in the [`Bytes`], this returns a view
    /// for that slice.
    ///
    /// This is useful if you're using some function that only returns a
    /// sub-slice `&[u8]` from a [`Bytes`], but you want to have that sub-slice
    /// as a view.
    fn view_from_slice(&self, subset: &[u8]) -> Option<Self> {
        if subset.is_empty() || self.len == 0 {
            Some(Self::default())
        }
        else {
            self.buf
                .subset(self.offset, self.len, subset)
                .and_then(|_start| {
                    // todo: view of start .. start + subset.len()
                    //self.view(sub_offset..(sub_offset + sub_len)).unwrap()
                    todo!();
                })
        }
    }

    #[inline]
    fn ref_count(&self) -> RefCount {
        unsafe { self.buf.0.ref_count() }
    }
}

#[derive(Clone, Copy)]
struct Buffer {
    /// Made from a `Box<[MaybeUninit<u8>]>>`
    ///
    /// This pointer is valid as long as the owning `Slab` exists, or a `Buffer`
    /// containing it exists. therefore, as long as you can acquire this
    /// `BufferPtr`, it's safe to assume that `buf` points to a valid
    /// memory.
    buf: *const [UnsafeCell<MaybeUninit<u8>>],

    /// Made from a `Box<AtomicUsize>`
    ///
    /// Invariant: This pointer is valid as long as the owning `Slab` exists, or
    /// a `Buffer` containing it exists. therefore, as long as you can
    /// acquire this `BufferPtr`, it's safe to assume that `inner` points to
    /// a valid `BufferInner`.
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

    #[inline]
    unsafe fn deallocate(self) {
        let _ref_count = Box::from_raw(self.ref_count as *mut AtomicUsize);
        let _buf = Box::from_raw(self.buf as *mut [UnsafeCell<MaybeUninit<u8>>]);
    }

    #[inline]
    unsafe fn ref_count(&self) -> RefCount {
        if self.ref_count.is_null() {
            RefCount::Static
        }
        else {
            unsafe { RefCount::from_atomic(&*self.ref_count) }
        }
    }
}

struct BufferOwned(Buffer);

impl BufferOwned {
    fn new(size: usize) -> (Self, BufferRef) {
        let buf = if size == 0 {
            Buffer::zero_sized()
        }
        else {
            // allocate ref_count
            let ref_count = Box::into_raw(Box::new(AtomicRefCount::new()));

            // allocate buffer
            let buf = Box::<[u8]>::new_uninit_slice(size);

            // leak it to raw pointer
            let buf = Box::into_raw(buf);

            // make it `*const [UnsafeCell<_>>]`.  This is roughly what
            // `UnsafeCell::from_mut` does.
            let buf = buf as *const [UnsafeCell<MaybeUninit<u8>>];

            Buffer { buf, ref_count }
        };

        // the ref count was initialized so that we can have one reference for the Slab,
        // and one make a BytesMut
        (Self(buf), BufferRef(buf))
    }

    fn get_ref(&self) -> BufferRef {
        if !self.0.ref_count.is_null() {
            unsafe {
                // SAFETY: See invariant on `BufferPtr`
                (*self.0.ref_count).increment();
            }
        }
        BufferRef(self.0)
    }
}

impl Drop for BufferOwned {
    fn drop(&mut self) {
        if !self.0.ref_count.is_null() {
            unsafe {
                if (*self.0.ref_count).orphan() {
                    self.0.deallocate();
                }
            }
        }
    }
}

impl BufferOwned {
    #[inline]
    fn try_reclaim(&self) -> bool {
        unsafe {
            // SAFETY: See invariant on `BufferPtr`
            (*self.0.ref_count).try_reclaim()
        }
    }

    #[inline]
    fn is_reclaimable(&self) -> bool {
        unsafe {
            // SAFETY: See invariant on `BufferPtr`
            (*self.0.ref_count).is_reclaimable()
        }
    }
}

struct BufferRef(Buffer);

impl BufferRef {
    /// # Safety
    ///
    /// The caller must ensure that there are no mutable references to this
    /// buffer, and that the range is valid.
    #[inline]
    unsafe fn bytes<'a>(&'a self, range: impl Into<Range>) -> &'a [MaybeUninit<u8>] {
        let ptr = self.0.buf.get_unchecked(range.into().as_slice_index());
        std::slice::from_raw_parts(UnsafeCell::raw_get(ptr.as_ptr()), ptr_len(ptr))
    }

    /// # Safety
    ///
    /// The caller must ensure that the access is unique, and that the range is
    /// valid. No other active references, mutable or not may exist to this
    /// slice.
    #[inline]
    unsafe fn bytes_mut<'a>(&'a self, range: impl Into<Range>) -> &'a mut [MaybeUninit<u8>] {
        let ptr = self.0.buf.get_unchecked(range.into().as_slice_index());
        std::slice::from_raw_parts_mut(UnsafeCell::raw_get(ptr.as_ptr()), ptr_len(ptr))
    }

    #[inline]
    fn len(&self) -> usize {
        ptr_len(self.0.buf)
    }

    fn subset(&self, offset: usize, len: usize, slice: &[u8]) -> Option<usize> {
        let bytes_ptr = self.0.buf as *const u8 as usize + offset;
        let sub_ptr = slice.as_ptr() as usize;
        let sub_len = slice.len();

        (sub_ptr >= bytes_ptr && sub_ptr + sub_len <= bytes_ptr + len).then(|| sub_ptr - bytes_ptr)
    }

    #[inline]
    fn is_exclusive(&self) -> bool {
        if !self.0.ref_count.is_null() {
            unsafe { (*self.0.ref_count).is_exclusive() }
        }
        else {
            // `ref_count` can only be null for zero-sized buffers. it's safe to have
            // multiple mutable references to those. They are just dangling pointers anyway.
            true
        }
    }
}

impl Default for BufferRef {
    #[inline]
    fn default() -> Self {
        Self(Buffer::zero_sized())
    }
}

impl Clone for BufferRef {
    fn clone(&self) -> Self {
        if !self.0.ref_count.is_null() {
            unsafe {
                // SAFETY: See invariant on `BufferPtr`
                (*self.0.ref_count).increment();
            }
        }

        Self(self.0)
    }
}

impl Drop for BufferRef {
    fn drop(&mut self) {
        if !self.0.ref_count.is_null() {
            unsafe {
                if (*self.0.ref_count).decrement() {
                    self.0.deallocate();
                }
            }
        }
    }
}

/// This manages the reference count of a [`Buffer`].
///
/// [`Buffers`] can be owned by a [`Slab`], or *orphaned*. This is through a
/// [`BufferOwned`]. This is encoded as the least significant bit.
///
/// [`Buffers`] can have any number of references through [`BufferRef`].
struct AtomicRefCount(AtomicUsize);

impl AtomicRefCount {
    #[inline]
    fn new() -> Self {
        Self(AtomicUsize::new(3))
    }

    #[inline]
    fn increment(&self) {
        self.0.fetch_add(2, Ordering::Relaxed);
    }

    /// Decrements reference count ([`BufferRef`]) and returns whether the
    /// buffer must be deallocated.
    #[inline]
    fn decrement(&self) -> bool {
        self.0.fetch_sub(2, Ordering::Relaxed) == 2
    }

    #[inline]
    fn is_exclusive(&self) -> bool {
        self.0.load(Ordering::Relaxed) & !1 == 2
    }

    /// Orphans the buffer and returns whether the buffer must be deallocated.
    #[inline]
    fn orphan(&self) -> bool {
        self.0.fetch_and(!1, Ordering::Relaxed) == 1
    }

    #[inline]
    fn try_reclaim(&self) -> bool {
        self.0
            .compare_exchange(1, 3, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }

    #[inline]
    fn is_reclaimable(&self) -> bool {
        self.0.load(Ordering::Relaxed) == 1
    }
}

#[derive(Clone, Copy, Debug)]
pub enum RefCount {
    Static,
    SlabManaged { ref_count: usize },
    Orphaned { ref_count: usize },
}

impl RefCount {
    fn from_atomic(value: &AtomicRefCount) -> Self {
        let value = value.0.load(Ordering::Relaxed);
        let ref_count = value >> 1;
        if value & 1 != 0 {
            Self::SlabManaged { ref_count }
        }
        else {
            Self::Orphaned { ref_count }
        }
    }

    #[inline]
    pub fn ref_count(&self) -> Option<usize> {
        match self {
            Self::Static => None,
            Self::SlabManaged { ref_count } | Self::Orphaned { ref_count } => Some(*ref_count),
        }
    }

    #[inline]
    pub fn is_orphaned(&self) -> bool {
        matches!(self, Self::Orphaned { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RefCount,
        Slab,
    };
    use crate::{
        buf::WriteError,
        Buf,
        BufMut,
        RangeOutOfBounds,
    };

    #[test]
    fn write_read_full_from_start() {
        let mut slab = Slab::new(128, 32);

        let mut bytes_mut = slab.get();
        bytes_mut.write(0..4, b"abcd", ..).unwrap();
        assert_eq!(bytes_mut.chunks(..).unwrap().next().unwrap(), b"abcd");
    }

    #[test]
    fn write_read_partial_from_start() {
        let mut slab = Slab::new(128, 32);

        let mut bytes_mut = slab.get();
        bytes_mut.write(0..4, b"abcd", ..).unwrap();
        assert_eq!(bytes_mut.chunks(0..2).unwrap().next().unwrap(), b"ab");
    }

    #[test]
    fn write_with_fill() {
        let mut slab = Slab::new(128, 32);

        let mut bytes_mut = slab.get();
        bytes_mut.write(4..8, b"abcd", ..).unwrap();
        assert_eq!(
            bytes_mut.chunks(..).unwrap().next().unwrap(),
            b"\x00\x00\x00\x00abcd"
        );
    }

    #[test]
    fn write_over_buf_end() {
        let mut slab = Slab::new(128, 32);

        let mut bytes_mut = slab.get();

        bytes_mut.write(0..4, b"abcd", ..).unwrap();
        bytes_mut.write(2..6, b"efgh", ..).unwrap();

        assert_eq!(bytes_mut.chunks(..).unwrap().next().unwrap(), b"abefgh");
    }

    #[test]
    fn write_extend_with_unbounded_destination_slice() {
        let mut slab = Slab::new(128, 32);

        let mut bytes_mut = slab.get();

        bytes_mut.write(0..4, b"abcd", ..).unwrap();
        bytes_mut.write(2.., b"efgh", ..).unwrap();

        assert_eq!(bytes_mut.chunks(..).unwrap().next().unwrap(), b"abefgh");
    }

    #[test]
    fn cant_read_more_than_written() {
        let mut slab = Slab::new(128, 32);

        let mut bytes_mut = slab.get();
        bytes_mut.write(0..4, b"abcd", 0..4).unwrap();

        assert_eq!(
            bytes_mut.chunks(0..8).unwrap_err(),
            RangeOutOfBounds {
                required: (0..8).into(),
                bounds: (0, 4)
            },
        );
    }

    #[test]
    fn cant_write_more_than_buf_size() {
        let mut slab = Slab::new(4, 32);

        let mut bytes_mut = slab.get();
        assert_eq!(
            bytes_mut.write(0..8, b"abcdefgh", 0..8).unwrap_err(),
            WriteError::Full {
                required: (0..8).into(),
                buf_length: 4
            }
        );
    }

    #[test]
    fn write_freeze_read() {
        let mut slab = Slab::new(128, 32);

        let mut bytes_mut = slab.get();
        bytes_mut.write(0..4, b"abcd", 0..4).unwrap();

        let bytes = bytes_mut.freeze();
        assert_eq!(bytes.chunks(..).unwrap().next().unwrap(), b"abcd");

        let bytes2 = bytes.clone();
        assert_eq!(bytes2.chunks(..).unwrap().next().unwrap(), b"abcd");
    }

    #[test]
    fn it_increments_ref_count_on_clone() {
        let mut slab = Slab::new(128, 32);
        let mut bytes_mut = slab.get();
        // if we don't write something into the buffer, we'll get a static (dangling,
        // zero-sized) buffer.
        bytes_mut.write(.., b"foobar", ..).unwrap();
        let bytes = bytes_mut.freeze();

        assert_eq!(bytes.ref_count().ref_count().unwrap(), 1);
        let bytes2 = bytes.clone();
        assert_eq!(bytes.ref_count().ref_count().unwrap(), 2);
        assert_eq!(bytes2.ref_count().ref_count().unwrap(), 2);
    }

    #[test]
    fn it_decrements_ref_count_on_drop() {
        let mut slab = Slab::new(128, 32);
        let mut bytes_mut = slab.get();
        // if we don't write something into the buffer, we'll get a static (dangling,
        // zero-sized) buffer.
        bytes_mut.write(.., b"foobar", ..).unwrap();
        let bytes = bytes_mut.freeze();
        let bytes2 = bytes.clone();

        assert_eq!(bytes.ref_count().ref_count().unwrap(), 2);
        assert_eq!(bytes2.ref_count().ref_count().unwrap(), 2);
        drop(bytes2);
        assert_eq!(bytes.ref_count().ref_count().unwrap(), 1);
    }

    #[test]
    fn it_orphanes_buffers() {
        let mut slab = Slab::new(128, 32);
        let bytes_mut = slab.get();

        assert!(!bytes_mut.ref_count().is_orphaned());
        drop(slab);
        assert!(bytes_mut.ref_count().is_orphaned());
    }

    #[test]
    fn empty_bytes_are_not_ref_counted() {
        let mut slab = Slab::new(128, 32);
        let bytes_mut = slab.get();
        let bytes = bytes_mut.freeze();

        assert!(matches!(bytes.ref_count(), RefCount::Static));
    }

    #[test]
    fn it_reuses_buffers() {
        let mut slab = Slab::new(128, 32);
        assert_eq!(slab.num_in_use(), 0);
        assert_eq!(slab.num_available(), 0);

        let bytes_mut = slab.get();
        let buf_ptr = bytes_mut.inner.buf.0.buf;
        assert_eq!(slab.num_in_use(), 1);
        assert_eq!(slab.num_available(), 0);

        drop(bytes_mut);
        let bytes_mut = slab.get();
        let buf2_ptr = bytes_mut.inner.buf.0.buf;
        assert_eq!(slab.num_in_use(), 1);
        assert_eq!(slab.num_available(), 0);

        assert_eq!(buf_ptr, buf2_ptr);
    }
}
