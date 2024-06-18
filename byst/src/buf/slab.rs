use crate::buf::arc_buf::{
    ArcBufMut,
    Reclaim,
};

/// Efficient allocation of equally-sized buffers.
pub struct Slab {
    buf_size: usize,
    reuse_count: usize,
    in_use: Vec<Reclaim>,
    available: Vec<(Reclaim, ArcBufMut)>,
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
    pub fn get(&mut self) -> ArcBufMut {
        if self.buf_size == 0 {
            return ArcBufMut::default();
        }

        if let Some((reclaim, buf)) = self.available.pop() {
            // there's a buffer in `available` that we can use.
            self.in_use.push(reclaim);
            buf
        }
        else {
            // try to find a buffer that is unused, but not yet in `available`.

            let mut i = 0;
            let mut reclaimed = None;

            while i < self.in_use.len() {
                let reclaim = &self.in_use[i];

                if reclaimed.is_none() {
                    // try to reclaim unused buffer

                    if let Some(buf) = reclaim.try_reclaim() {
                        reclaimed = Some(buf);
                    }
                    else {
                        i += 1;
                    }
                }
                else {
                    // get all other buffers that are reclaimable from `in_use` and put them into
                    // `available`, or drop them.

                    if let Some(buf) = reclaim.try_reclaim() {
                        let reclaim = self.in_use.swap_remove(i);
                        if self.available.len() < self.reuse_count {
                            // put buffer into available list
                            self.available.push((reclaim, buf));
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
                let (buf, reclaim) = ArcBufMut::new_reclaimable(self.buf_size);
                self.in_use.push(reclaim);
                buf
            }
        }
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

#[cfg(test)]
mod tests {
    use super::Slab;
    use crate::{
        buf::{
            arc_buf::RefCount,
            copy::{
                copy,
                CopyError,
            },
            Full,
        },
        Buf,
        RangeOutOfBounds,
    };

    #[test]
    fn write_read_full_from_start() {
        let mut slab = Slab::new(128, 32);

        let mut bytes_mut = slab.get();
        copy(&mut bytes_mut, 0..4, b"abcd", ..).unwrap();
        assert_eq!(bytes_mut.chunks(..).unwrap().next().unwrap(), b"abcd");
    }

    #[test]
    fn write_read_partial_from_start() {
        let mut slab = Slab::new(128, 32);

        let mut bytes_mut = slab.get();
        copy(&mut bytes_mut, 0..4, b"abcd", ..).unwrap();
        assert_eq!(bytes_mut.chunks(0..2).unwrap().next().unwrap(), b"ab");
    }

    #[test]
    fn write_with_fill() {
        let mut slab = Slab::new(128, 32);

        let mut bytes_mut = slab.get();
        copy(&mut bytes_mut, 4..8, b"abcd", ..).unwrap();
        assert_eq!(
            bytes_mut.chunks(..).unwrap().next().unwrap(),
            b"\x00\x00\x00\x00abcd"
        );
    }

    #[test]
    fn write_over_buf_end() {
        let mut slab = Slab::new(128, 32);

        let mut bytes_mut = slab.get();

        copy(&mut bytes_mut, 0..4, b"abcd", ..).unwrap();
        copy(&mut bytes_mut, 2..6, b"efgh", ..).unwrap();

        assert_eq!(bytes_mut.chunks(..).unwrap().next().unwrap(), b"abefgh");
    }

    #[test]
    fn write_extend_with_unbounded_destination_slice() {
        let mut slab = Slab::new(128, 32);

        let mut bytes_mut = slab.get();

        copy(&mut bytes_mut, 0..4, b"abcd", ..).unwrap();
        copy(&mut bytes_mut, 2.., b"efgh", ..).unwrap();

        assert_eq!(bytes_mut.chunks(..).unwrap().next().unwrap(), b"abefgh");
    }

    #[test]
    fn cant_read_more_than_written() {
        let mut slab = Slab::new(128, 32);

        let mut bytes_mut = slab.get();
        copy(&mut bytes_mut, 0..4, b"abcd", 0..4).unwrap();

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
            copy(&mut bytes_mut, 0..8, b"abcdefgh", 0..8).unwrap_err(),
            CopyError::Full(Full {
                required: 8,
                capacity: 4
            })
        );
    }

    #[test]
    fn write_freeze_read() {
        let mut slab = Slab::new(128, 32);

        let mut bytes_mut = slab.get();
        copy(&mut bytes_mut, 0..4, b"abcd", 0..4).unwrap();

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
        copy(&mut bytes_mut, .., b"foobar", ..).unwrap();
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
        copy(&mut bytes_mut, .., b"foobar", ..).unwrap();
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

        assert!(bytes_mut.ref_count().can_be_reclaimed());
        drop(slab);
        assert!(!bytes_mut.ref_count().can_be_reclaimed());
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
        //let buf_ptr = bytes_mut.inner.buf.0.buf;
        assert_eq!(slab.num_in_use(), 1);
        assert_eq!(slab.num_available(), 0);

        drop(bytes_mut);
        let _bytes_mut = slab.get();
        //let buf2_ptr = bytes_mut.inner.buf.0.buf;
        assert_eq!(slab.num_in_use(), 1);
        assert_eq!(slab.num_available(), 0);

        //assert_eq!(buf_ptr, buf2_ptr);
    }
}
