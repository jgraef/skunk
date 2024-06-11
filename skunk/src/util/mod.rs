//! Utilities.

pub(crate) mod boolean;
pub mod bytes;
pub(crate) mod error;
pub mod io;
pub mod zc;

use std::{
    fmt::Debug,
    iter::FusedIterator,
    num::NonZeroUsize,
    sync::{
        atomic::{
            AtomicUsize,
            Ordering,
        },
        Arc,
    },
};

use parking_lot::Mutex;
pub use skunk_macros::for_tuple;
pub use tokio_util::sync::CancellationToken;

/// [`Oncelock`](std::sync::OnceLock::get_or_try_init) is not stabilized yet, so
/// we implement it ourselves. Also we inclose the `Arc`, because why not.
pub struct Lazy<T>(Mutex<Option<Arc<T>>>);

impl<T> Lazy<T> {
    pub const fn new() -> Self {
        Self(Mutex::new(None))
    }

    pub fn get_or_try_init<E>(&self, f: impl FnOnce() -> Result<T, E>) -> Result<Arc<T>, E> {
        let mut guard = self.0.lock();
        if let Some(value) = &*guard {
            Ok(value.clone())
        }
        else {
            let value = Arc::new(f()?);
            *guard = Some(value.clone());
            Ok(value)
        }
    }
}

/// ID generator. Generates sequential [`NonZeroUsize`]s starting with 1.
/// This uses an [`AtomicUsize`] internally, so it doesn't require mutable
/// access to self to increment the counter.
#[derive(Debug)]
pub struct UsizeIdGenerator {
    next: AtomicUsize,
}

impl Default for UsizeIdGenerator {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl UsizeIdGenerator {
    #[inline]
    pub const fn new() -> Self {
        Self {
            next: AtomicUsize::new(1),
        }
    }

    #[inline]
    pub fn next(&self) -> NonZeroUsize {
        NonZeroUsize::new(self.next.fetch_add(1, Ordering::Relaxed)).expect("id overflow")
    }
}

/// Returns a general unique ID. These IDs are unique for the whole program.
pub fn unique_id() -> NonZeroUsize {
    unique_ids!()
}

/// Creates a static ID generator and returns the next ID. These IDs are unique
/// per macro call-site. IDs are of type [`NonZeroUsize`].
///
/// # Note
///
/// When used in a generic scope, this will provide unique IDs for *all*
/// instantiations of the scope, *not* each one.
///
/// > A static item defined in a generic scope (for example in a blanket or
/// > default implementation) will result in exactly one static item being
/// > defined, as if the static definition was pulled out of the current scope
/// > into the module. There will not be one item per monomorphization.
/// ([source](https://doc.rust-lang.org/reference/items/static-items.html#statics--generics))
macro_rules! unique_ids {
    () => {{
        static UNIQUE_IDS: crate::util::UsizeIdGenerator = crate::util::UsizeIdGenerator::new();
        UNIQUE_IDS.next()
    }};
}

pub(crate) use unique_ids;

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
