//! # Note
//!
//! This is intentionally not public. If used with a type `B`, which doesn't
//! uphold some invariants, can cause UB. We could make the `new` method unsafe,
//! or use an unsafe-marked trait.

use std::{
    fmt::Debug,
    mem::MaybeUninit,
};

use super::{
    Full,
    Length,
};
use crate::{
    buf::{
        Buf,
        BufMut,
        SizeLimit,
    },
    impl_me,
    io::BufWriter,
    range::{
        Range,
        RangeOutOfBounds,
    },
    util::debug_as_hexdump,
};

/// A contiguous chunk of memory that is partially initialized.
#[derive(Copy, Clone)]
pub struct PartiallyInitialized<B> {
    // invariant: `buf[..initialized]` is initialized.
    buf: B,
    initialized: usize,
}

impl<B> PartiallyInitialized<B> {
    #[inline]
    pub fn new(buf: B) -> Self {
        Self {
            buf,
            initialized: 0,
        }
    }

    #[inline]
    pub unsafe fn assume_initialized(&mut self, initialized: usize) {
        self.initialized = self.initialized.max(initialized);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.initialized = 0;
    }

    #[inline]
    pub fn inner_ref(&self) -> &B {
        &self.buf
    }

    #[inline]
    pub fn inner_mut(&mut self) -> &mut B {
        &mut self.buf
    }

    #[inline]
    pub fn into_parts(self) -> (B, usize) {
        (self.buf, self.initialized)
    }
}

impl<B: AsRef<[MaybeUninit<u8>]>> PartiallyInitialized<B> {
    #[inline]
    fn bytes(&self) -> &[u8] {
        // invariant: this will always return a slice of length `self.initialized`.

        // SAFETY: see invariant in struct
        unsafe { MaybeUninit::slice_assume_init_ref(&self.buf.as_ref()[..self.initialized]) }
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.buf.as_ref().len()
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.initialized == self.buf.as_ref().len()
    }
}

impl<B: AsRef<[MaybeUninit<u8>]> + AsMut<[MaybeUninit<u8>]>> PartiallyInitialized<B> {
    #[inline]
    fn bytes_mut(&mut self) -> &mut [u8] {
        // invariant: this will always return a slice of length `self.initialized`.

        // SAFETY: see invariant in struct
        unsafe { MaybeUninit::slice_assume_init_mut(&mut self.buf.as_mut()[..self.initialized]) }
    }

    pub fn resize(&mut self, new_len: usize, value: u8) {
        let n = self.buf.as_ref().len();
        if new_len > n {
            panic!("Can't resize ArrayBuf<{n}> to length {new_len}");
        }

        // after this the struct invariant still holds
        if new_len > self.initialized {
            MaybeUninit::fill(&mut self.buf.as_mut()[self.initialized..new_len], value);
        }
        self.initialized = new_len;
    }
}

impl<B: AsRef<[MaybeUninit<u8>]>> AsRef<[u8]> for PartiallyInitialized<B> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.bytes()
    }
}

impl<B: AsRef<[MaybeUninit<u8>]> + AsMut<[MaybeUninit<u8>]>> AsMut<[u8]>
    for PartiallyInitialized<B>
{
    #[inline]
    fn as_mut(&mut self) -> &mut [u8] {
        self.bytes_mut()
    }
}

impl<B: AsRef<[MaybeUninit<u8>]>> Debug for PartiallyInitialized<B> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        debug_as_hexdump(f, self)
    }
}

impl<B: AsRef<[MaybeUninit<u8>]>> PartialEq for PartiallyInitialized<B> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.bytes() == other.bytes()
    }
}

impl<B: AsRef<[MaybeUninit<u8>]>> Eq for PartiallyInitialized<B> {}

impl<B: AsRef<[MaybeUninit<u8>]>> Buf for PartiallyInitialized<B> {
    type View<'a> = &'a [u8]
    where
        Self: 'a;

    type Reader<'a> = &'a [u8]
    where
        Self: 'a;

    #[inline]
    fn view(&self, range: impl Into<Range>) -> Result<Self::View<'_>, RangeOutOfBounds> {
        range.into().slice_get(self.bytes())
    }

    #[inline]
    fn reader(&self) -> Self::Reader<'_> {
        self.bytes()
    }
}

impl<B: AsRef<[MaybeUninit<u8>]>> Length for PartiallyInitialized<B> {
    #[inline]
    fn len(&self) -> usize {
        self.initialized
    }
}

impl<B: AsRef<[MaybeUninit<u8>]> + AsMut<[MaybeUninit<u8>]>> BufMut for PartiallyInitialized<B> {
    type ViewMut<'a> = &'a mut [u8]
    where
        Self: 'a;

    type Writer<'a> = PartiallyInitializedWriter<'a, B>
    where
        Self: 'a;

    #[inline]
    fn view_mut(&mut self, range: impl Into<Range>) -> Result<Self::ViewMut<'_>, RangeOutOfBounds> {
        range.into().slice_get_mut(self.bytes_mut())
    }

    #[inline]
    fn writer(&mut self) -> Self::Writer<'_> {
        PartiallyInitializedWriter {
            partially_initialized: self,
            position: 0,
        }
    }

    fn reserve(&mut self, size: usize) -> Result<(), Full> {
        let n = self.buf.as_ref().len();
        if size <= n {
            Ok(())
        }
        else {
            Err(Full {
                required: size,
                capacity: n,
            })
        }
    }

    #[inline]
    fn size_limit(&self) -> SizeLimit {
        self.buf.as_ref().len().into()
    }
}

pub struct PartiallyInitializedWriter<'a, B> {
    partially_initialized: &'a mut PartiallyInitialized<B>,
    position: usize,
}

impl<'a, B: AsRef<[MaybeUninit<u8>]> + AsMut<[MaybeUninit<u8>]>> PartiallyInitializedWriter<'a, B> {
    /// Fills the next `length` bytes by applying the closure `f` to it.
    ///
    /// # Safety
    ///
    /// `f` must initialize make sure the whole slice it is passed is
    /// initialized after its call.
    unsafe fn fill_with(
        &mut self,
        length: usize,
        f: impl FnOnce(&mut [MaybeUninit<u8>]),
    ) -> Result<(), Full> {
        let end = self.position + length;

        if end <= self.partially_initialized.capacity() {
            f(&mut self.partially_initialized.buf.as_mut()[self.position..end]);

            unsafe {
                // SAFETY:
                //  - access is unqiue, because we have a `&mut self`
                //  - the bytes upto `end` have just been initialized
                self.partially_initialized.assume_initialized(end);
            }

            self.position = end;

            Ok(())
        }
        else {
            Err(Full {
                required: length,
                capacity: self.partially_initialized.capacity(),
            })
        }
    }
}

impl<'b, B: AsRef<[MaybeUninit<u8>]> + AsMut<[MaybeUninit<u8>]>> BufWriter
    for PartiallyInitializedWriter<'b, B>
{
    type ViewMut<'a> = &'a mut [u8] where Self: 'a;

    #[inline]
    fn peek_chunk_mut(&mut self) -> Option<&mut [u8]> {
        if self.position < self.partially_initialized.initialized {
            Some(&mut self.partially_initialized.bytes_mut()[self.position..])
        }
        else {
            None
        }
    }

    fn view_mut(&mut self, length: usize) -> Result<Self::ViewMut<'_>, crate::io::Full> {
        if self.position + length <= self.partially_initialized.initialized {
            let view = &mut self.partially_initialized.bytes_mut()[self.position..][..length];
            self.position += length;
            Ok(view)
        }
        else {
            Err(crate::io::Full {
                written: 0,
                requested: length,
                remaining: self.partially_initialized.initialized - self.position,
            })
        }
    }

    #[inline]
    fn peek_view_mut(&mut self, length: usize) -> Result<Self::ViewMut<'_>, crate::io::Full> {
        if self.position + length <= self.partially_initialized.initialized {
            Ok(&mut self.partially_initialized.bytes_mut()[self.position..][..length])
        }
        else {
            Err(crate::io::Full {
                written: 0,
                requested: length,
                remaining: self.partially_initialized.initialized - self.position,
            })
        }
    }

    #[inline]
    fn rest_mut(&mut self) -> Self::ViewMut<'_> {
        let new_position = self.partially_initialized.initialized;
        let rest = &mut self.partially_initialized.bytes_mut()[self.position..];
        self.position = new_position;
        rest
    }

    #[inline]
    fn peek_rest_mut(&mut self) -> Self::ViewMut<'_> {
        &mut self.partially_initialized.bytes_mut()[self.position..]
    }

    #[inline]
    fn advance(&mut self, by: usize) -> Result<(), crate::io::Full> {
        // note: The cursor position can't be greater than `self.filled`, and both point
        // into the initialized portion, so it's safe to assume that the buffer has been
        // initialized upto `already_filled`.
        let already_filled = self.partially_initialized.initialized - self.position;

        if by > already_filled {
            unsafe {
                // SAFETY: The closure initializes `already_filled..`. `..already_filled` is
                // already filled, and thus initialized.
                self.fill_with(by, |buf| {
                    MaybeUninit::fill(&mut buf[already_filled..], 0);
                })
            }
            .map_err(Into::into)
        }
        else {
            self.position += by;
            Ok(())
        }
    }

    #[inline]
    fn remaining(&self) -> usize {
        self.partially_initialized.initialized - self.position
    }

    #[inline]
    fn extend(&mut self, with: &[u8]) -> Result<(), crate::io::Full> {
        unsafe {
            // SAFETY: The closure initializes the whole slice.
            self.fill_with(with.len(), |buf| {
                MaybeUninit::copy_from_slice(buf, with);
            })
        }
        .map_err(Into::into)
    }
}

impl_me! {
    impl['a, B: AsRef<[MaybeUninit<u8>]> + AsMut<[MaybeUninit<u8>]>] Writer for PartiallyInitializedWriter<'a, B> as BufWriter;
}
