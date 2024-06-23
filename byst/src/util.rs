use std::{
    fmt::{
        Debug,
        Display,
    },
    iter::FusedIterator,
};

pub use byst_macros::for_tuple;

use crate::{
    io::{
        BufReader,
        End,
    },
    Buf,
};

pub struct Peekable<I: Iterator> {
    pub inner: I,
    pub peeked: Option<I::Item>,
    pub peeked_back: Option<I::Item>,
}

impl<I: Iterator> Peekable<I> {
    #[inline]
    pub fn new(inner: I) -> Self {
        Self {
            inner,
            peeked: None,
            peeked_back: None,
        }
    }

    #[inline]
    pub fn peek(&mut self) -> Option<&I::Item> {
        self.peek_inner();
        // if `self.peeked` is None, we're done iterating the inner iterator, but might
        // have peeked from the other side. In that case that will be the next item.
        self.peeked.as_ref().or_else(|| self.peeked_back.as_ref())
    }

    #[inline]
    pub fn peek_mut(&mut self) -> Option<&mut I::Item> {
        self.peek_inner();
        // if `self.peeked` is None, we're done iterating the inner iterator, but might
        // have peeked from the other side. In that case that will be the next item.
        self.peeked.as_mut().or_else(|| self.peeked_back.as_mut())
    }

    #[inline]
    fn peek_inner(&mut self) {
        if self.peeked.is_none() {
            self.peeked = self.next_inner();
        }
    }

    #[inline]
    fn next_inner(&mut self) -> Option<I::Item> {
        self.inner.next()
    }
}

impl<I: DoubleEndedIterator> Peekable<I> {
    #[inline]
    pub fn peek_back(&mut self) -> Option<&I::Item> {
        self.peek_back_inner();
        // if `self.peeked` is None, we're done iterating the inner iterator, but might
        // have peeked from the other side. In that case that will be the next item.
        self.peeked_back.as_ref().or_else(|| self.peeked.as_ref())
    }

    #[inline]
    pub fn peek_back_mut(&mut self) -> Option<&mut I::Item> {
        self.peek_back_inner();
        // if `self.peeked` is None, we're done iterating the inner iterator, but might
        // have peeked from the other side. In that case that will be the next item.
        self.peeked_back.as_mut().or_else(|| self.peeked.as_mut())
    }

    #[inline]
    fn peek_back_inner(&mut self) {
        if self.peeked_back.is_none() {
            self.peeked_back = self.next_back_inner();
        }
    }

    #[inline]
    fn next_back_inner(&mut self) -> Option<I::Item> {
        self.inner.next_back()
    }
}

impl<I: Iterator> Iterator for Peekable<I> {
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.peeked
            .take()
            .or_else(|| self.next_inner())
            // if we don't have a peeked value and the inner iterator is done, we might still have a
            // peeked value from the other side. We'll return that in this case.
            .or_else(|| self.peeked_back.take())
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (min, max) = self.inner.size_hint();
        let peek_count = self.peeked.is_some().then_some(1).unwrap_or_default()
            + self.peeked_back.is_some().then_some(1).unwrap_or_default();
        (min + peek_count, max.map(|max| max + peek_count))
    }
}

impl<I: DoubleEndedIterator> DoubleEndedIterator for Peekable<I> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.peeked_back
            .take()
            .or_else(|| self.next_back_inner())
            // if we don't have a peeked value and the inner iterator is done, we might still have a
            // peeked value from the other side. We'll return that in this case.
            .or_else(|| self.peeked.take())
    }
}

impl<I: Iterator + ExactSizeIterator> ExactSizeIterator for Peekable<I> {}

impl<I: Iterator> FusedIterator for Peekable<I> {}

impl<I: Iterator + Debug> Debug for Peekable<I>
where
    I::Item: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Peekable")
            .field("inner", &self.inner)
            .field("peeked", &self.peeked)
            .field("peeked_back", &self.peeked_back)
            .finish()
    }
}

#[derive(Debug)]
pub struct Map<I, M> {
    inner: I,
    map: M,
}

impl<I, M> Map<I, M> {
    #[inline]
    pub fn new(inner: I, map: M) -> Self {
        Self { inner, map }
    }
}

impl<I: Iterator, M: MapFunc<I::Item>> Iterator for Map<I, M> {
    type Item = M::Output;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let item = self.inner.next()?;
        Some(self.map.map(item))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<I: DoubleEndedIterator, M: MapFunc<I::Item>> DoubleEndedIterator for Map<I, M> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        let item = self.inner.next_back()?;
        Some(self.map.map(item))
    }
}

impl<I: ExactSizeIterator, M: MapFunc<I::Item>> ExactSizeIterator for Map<I, M> {}

impl<I: FusedIterator, M: MapFunc<I::Item>> FusedIterator for Map<I, M> {}

pub trait MapFunc<T> {
    type Output;

    fn map(&mut self, input: T) -> Self::Output;
}

#[derive(Debug)]
pub struct ExactSizeIter<I> {
    inner: I,
    exact_size: usize,
}

impl<I> ExactSizeIter<I> {
    #[inline]
    pub fn new(inner: I, exact_size: usize) -> Self {
        Self { inner, exact_size }
    }
}

impl<I: Iterator> Iterator for ExactSizeIter<I> {
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let item = self.inner.next()?;
        self.exact_size -= 1;
        Some(item)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.exact_size, Some(self.exact_size))
    }
}

impl<I: DoubleEndedIterator> DoubleEndedIterator for ExactSizeIter<I> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        let item = self.inner.next_back()?;
        self.exact_size -= 1;
        Some(item)
    }
}

impl<I: Iterator> ExactSizeIterator for ExactSizeIter<I> {}

impl<I: FusedIterator> FusedIterator for ExactSizeIter<I> {}

pub struct IsEndIter<I: Iterator> {
    inner: Peekable<I>,
    iterated_from_start: bool,
    iterated_from_end: bool,
}

impl<I: Iterator> IsEndIter<I> {
    #[inline]
    pub fn new(inner: I) -> Self {
        Self {
            inner: Peekable::new(inner),
            iterated_from_end: false,
            iterated_from_start: false,
        }
    }
}

impl<I: Iterator> Iterator for IsEndIter<I> {
    type Item = IsEnd<I::Item>;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.inner.next()?;
        let is_start = !self.iterated_from_start;
        let is_end = !self.iterated_from_end && self.inner.peek().is_none();
        self.iterated_from_start = true;
        Some(IsEnd {
            is_start,
            is_end,
            item,
        })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<I: DoubleEndedIterator> DoubleEndedIterator for IsEndIter<I> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let item = self.inner.next_back()?;
        let is_end = !self.iterated_from_end;
        let is_start = !self.iterated_from_start && self.inner.peek_back().is_none();
        self.iterated_from_end = true;
        Some(IsEnd {
            is_start,
            is_end,
            item,
        })
    }
}

impl<I: ExactSizeIterator> ExactSizeIterator for IsEndIter<I> {}

impl<I: FusedIterator> FusedIterator for IsEndIter<I> {}

impl<I: Iterator + Debug> Debug for IsEndIter<I>
where
    I::Item: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IsEndIter")
            .field("inner", &self.inner)
            .field("iterated_from_start", &self.iterated_from_start)
            .field("iterated_from_end", &self.iterated_from_end)
            .finish()
    }
}

#[derive(Debug)]
pub struct IsEnd<T> {
    pub is_start: bool,
    pub is_end: bool,
    pub item: T,
}

pub fn debug_as_hexdump(f: &mut std::fmt::Formatter, buf: impl Buf) -> std::fmt::Result {
    use crate::hexdump::{
        Config,
        Hexdump,
    };
    let hex = Hexdump::with_config(
        buf,
        Config {
            offset: 0,
            trailing_newline: false,
            at_least_one_line: false,
            header: false,
        },
    );
    Display::fmt(&hex, f)
}

/// Checks if `needle` is a sub-slice of `haystack`, and returns the index at
/// which `needle` starts in `haystack`.
pub fn sub_slice_index(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    let haystack_start = haystack.as_ptr() as usize;
    let haystack_end = haystack_start + haystack.len();
    let needle_start = needle.as_ptr() as usize;
    let needle_end = needle_start + needle.len();

    (needle_start >= haystack_start && needle_end <= haystack_end)
        .then(|| needle_start - haystack_start)
}

pub fn buf_eq(left: impl Buf, right: impl Buf) -> bool {
    let left_len = left.len();
    let right_len = right.len();

    if left_len != right_len {
        return false;
    }

    if left_len == 0 {
        // this also means right_len == 0
        return true;
    }

    let mut left_reader = left.reader();
    let mut right_reader = right.reader();

    loop {
        match (left_reader.chunk(), right_reader.chunk()) {
            (Err(End), Err(End)) => {
                // boths chunks are exhausted at the same time, and they haven't been unequal
                // yet. thus they're equal.
                break true;
            }
            (Ok(_), Err(End)) | (Err(End), Ok(_)) => {
                // we checked that both bufs have the same length, so this should not happen.

                panic!(
                    "Both bufs have same length ({left_len}), but readers read different amount of bytes."
                );
            }
            (Ok(left), Ok(right)) => {
                let left_len = <[u8]>::len(left);
                let right_len = <[u8]>::len(right);
                let n = std::cmp::min(left_len, right_len);

                if left[..n] != right[..n] {
                    break false;
                }

                left_reader.advance(n).unwrap_or_else(|End| {
                    panic!(
                        "Reader returned chunk of length {left_len}, but failed to advance by {n}."
                    )
                });
                right_reader.advance(n).unwrap_or_else(|End| {
                    panic!(
                        "Reader returned chunk of length {right_len}, but failed to advance by {n}."
                    )
                });
            }
        }
    }
}

macro_rules! cfg_pub {
    {
        $(#[$attr:meta])*
        pub(#[cfg($($cfg:tt)*)]) $($rest:tt)*
    } => {
        #[cfg($($cfg)*)]
        $(#[$attr])*
        pub $($rest)*

        #[cfg(not($($cfg)*))]
        $(#[$attr])*
        pub(crate) $($rest)*
    };
}
pub(crate) use cfg_pub;

// todo: make this a proc-macro
#[macro_export]
macro_rules! impl_me {
    {} => {};
    {
        impl $([ $($generics:tt)* ])? Reader for $ty:ty as BufReader;
        $($rest:tt)*
    } => {
        impl<$($($generics)*)?> ::byst::io::Reader for $ty {
            #[inline]
            fn read_into<
                D: ::byst::buf::BufMut
            >(
                &mut self,
                mut dest: D,
                limit: impl Into<Option<usize>>
            ) -> usize {
                ::byst::copy_io(dest.writer(), self, limit)
            }

            #[inline]
            fn skip(&mut self, amount: usize) -> usize {
                if let Err(::byst::io::End) = ::byst::io::BufReader::advance(self, amount) {
                    let remaining = ::byst::io::BufReader::remaining(self);
                    *self = Default::default();
                    remaining
                }
                else {
                    amount
                }
            }
        }

        impl<$($($generics)*)?> ::byst::io::Remaining for $ty {
            #[inline]
            fn remaining(&self) -> usize {
                ::byst::io::BufReader::remaining(self)
            }
        }

        /* fixme: this causes conflicting impls. we can let the macro caller specify the view type
        impl<$($($generics)*)?> ::byst::io::Read<$ty, ::byst::io::Length> for <$ty as ::byst::io::BufReader>::View {
            type Error = ::byst::io::End;

            #[inline]
            fn read(reader: &mut $ty, length: ::byst::io::Length) -> Result<Self, Self::Error> {
                let view = ::byst::io::BufReader::view(reader, length.0)?;
                ::byst::io::BufReader::advance(reader, length.0)?;
                Ok(view)
            }
        }
        */

        impl_me!{ $($rest)* }
    };
    {
        impl $([ $($generics:tt)* ])? Read<Self, ()> for $ty:ty as BufReader;
        $($rest:tt)*
    } => {
        impl<$($($generics)*)?> ::byst::io::Read<$ty, ()> for $ty {
            // todo: this should be Infallible
            type Error = ::byst::io::End;

            #[inline]
            fn read(reader: &mut $ty, _context: ()) -> Result<Self, Self::Error> {
                Ok(std::mem::take(reader))
            }
        }

        impl_me!{ $($rest)* }
    };
    {
        impl $([ $($generics:tt)* ])? Read<_, ()> for $ty:ty as BufReader;
        $($rest:tt)*
    } => {
        impl<$($($generics)*,)? __R> ::byst::io::Read<__R, ()> for $ty
        where
            __R: ::byst::io::BufReader<View = $ty>,
        {
            // todo: this should be Infallible
            type Error = ::byst::io::End;

            #[inline]
            fn read(reader: &mut __R, _context: ()) -> Result<Self, Self::Error> {
                let view = reader.rest();
                if view.is_empty() {
                    Err(byst::io::End)
                }
                else {
                    Ok(view)
                }
            }
        }

        impl_me!{ $($rest)* }
    };
}
pub use impl_me;

#[cfg(test)]
mod tests {
    use super::buf_eq;
    use crate::buf::rope::Rope;

    #[test]
    fn buf_eq_returns_false_for_different_lengths() {
        assert!(!buf_eq(b"Hello", b"Wor"));
        assert!(!buf_eq(b"", b"Hello"));
        assert!(!buf_eq(b"Hello", b""));
    }

    #[test]
    fn buf_eq_returns_false_for_same_length_buf_different_bytes() {
        assert!(!buf_eq(b"Hello", b"World"));
        assert!(!buf_eq(b"Hello", b"HellO"));
    }

    #[test]
    fn buf_eq_returns_true_for_empty_buffers() {
        assert!(buf_eq(b"", b""));
    }

    #[test]
    fn buf_eq_returns_true_for_equal_buffers() {
        assert!(buf_eq(b"Hello", b"Hello"));
    }

    #[test]
    fn buf_eq_returns_true_for_same_contents_but_differently_sized_chunks() {
        let mut buf1 = Rope::with_capacity(2);
        buf1.push(b"Hello" as &[u8]);
        buf1.push(b" World" as &[u8]);
        let mut buf2 = Rope::with_capacity(2);
        buf2.push(b"Hel" as &[u8]);
        buf2.push(b"lo World" as &[u8]);
        assert!(buf_eq(buf1, buf2));
    }

    #[test]
    fn buf_eq_returns_true_even_if_with_empty_chunks() {
        let mut buf1 = Rope::with_capacity(5);
        buf1.push(b"" as &[u8]);
        buf1.push(b"Hello" as &[u8]);
        buf1.push(b"" as &[u8]);
        buf1.push(b"World" as &[u8]);
        buf1.push(b"" as &[u8]);
        let mut buf2 = Rope::with_capacity(2);
        buf2.push(b"Hello" as &[u8]);
        buf2.push(b"World" as &[u8]);
        assert!(buf_eq(buf1, buf2));
    }
}
