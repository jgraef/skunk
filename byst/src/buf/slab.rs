use crate::buf::arc_buf::{
    ArcBufMut,
    Reclaim,
};

/// Efficient allocation of equally-sized buffers.
#[derive(Debug)]
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

                    if let Some(mut buf) = reclaim.try_reclaim() {
                        let reclaim = self.in_use.swap_remove(i);
                        if self.available.len() < self.reuse_count {
                            // put buffer into available list
                            buf.clear();
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
            tests::buf_mut_tests,
            BufExt,
            Full,
        },
        copy,
        Buf,
        RangeOutOfBounds,
    };

    buf_mut_tests!({
        let mut slab = Slab::new(128, 32);
        slab.get()
    });

    #[test]
    fn cant_read_more_than_written() {
        let mut slab = Slab::new(128, 32);

        let mut bytes_mut = slab.get();
        copy(&mut bytes_mut, b"abcd").unwrap();

        assert_eq!(
            bytes_mut.view(0..8).unwrap_err(),
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
            copy(&mut bytes_mut, b"abcdefgh").unwrap_err(),
            Full {
                required: 8,
                capacity: 4
            }
        );
    }

    #[test]
    fn write_freeze_read() {
        let mut slab = Slab::new(128, 32);

        let mut bytes_mut = slab.get();
        copy(&mut bytes_mut, b"abcd").unwrap();

        let bytes = bytes_mut.freeze();
        assert_eq!(bytes.into_vec(), b"abcd");

        let bytes2 = bytes.clone();
        assert_eq!(bytes2.into_vec(), b"abcd");
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

    #[test]
    fn it_doesnt_reuse_in_use_buffer() {
        let mut slab = Slab::new(128, 32);

        let bytes_mut = slab.get();
        let bytes_mut2 = slab.get();
        assert_eq!(bytes_mut.ref_count().ref_count(), Some(1));
        assert_eq!(bytes_mut2.ref_count().ref_count(), Some(1));
    }
}
