use std::iter::FusedIterator;

pub use byst_macros::for_tuple;

#[inline]
pub(crate) fn ptr_len<T>(ptr: *const [T]) -> usize {
    let ptr: *const [()] = ptr as _;
    // SAFETY: There is no aliasing as () is zero-sized
    let slice: &[()] = unsafe { &*ptr };
    slice.len()
}

pub struct Peekable<I: Iterator> {
    inner: I,
    peeked: Option<I::Item>,
    exhausted: bool,
}

impl<I: Iterator> Peekable<I> {
    pub fn new(inner: I) -> Self {
        Self {
            inner,
            peeked: None,
            exhausted: false,
        }
    }

    pub fn with_peeked(inner: I, peeked: I::Item) -> Self {
        Self {
            inner,
            peeked: Some(peeked),
            exhausted: false,
        }
    }

    pub fn into_parts(self) -> (I, Option<I::Item>) {
        (self.inner, self.peeked)
    }

    pub fn peek(&mut self) -> Option<&I::Item> {
        self.peek_inner();
        self.peeked.as_ref()
    }

    pub fn peek_mut(&mut self) -> Option<&mut I::Item> {
        self.peek_inner();
        self.peeked.as_mut()
    }

    fn peek_inner(&mut self) {
        if self.peeked.is_none() {
            self.peeked = self.next_inner();
        }
    }

    fn next_inner(&mut self) -> Option<I::Item> {
        if self.exhausted {
            None
        }
        else {
            let next = self.inner.next();
            if next.is_none() {
                self.exhausted = true;
            }
            next
        }
    }
}

impl<I: Iterator> Iterator for Peekable<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(peeked) = self.peeked.take() {
            Some(peeked)
        }
        else {
            self.next_inner()
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (min, max) = self.inner.size_hint();
        let peek_count = self.peeked.is_some().then_some(1).unwrap_or_default();
        (min + peek_count, max.map(|max| max + peek_count))
    }
}

impl<I: Iterator + ExactSizeIterator> ExactSizeIterator for Peekable<I> {}

impl<I: Iterator> FusedIterator for Peekable<I> {}
