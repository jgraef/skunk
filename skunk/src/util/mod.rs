//! Utilities.

pub(crate) mod arena;
pub(crate) mod bool_expr;
pub(crate) mod error;
pub mod io;

use std::{
    fmt::{
        Debug,
        Formatter,
    },
    hash::{
        Hash,
        Hasher,
    },
    num::NonZeroUsize,
    ops::Deref,
    sync::{
        Arc,
        Mutex,
    },
};

pub use bytes;
use bytes::Bytes;
pub use tokio_util::sync::CancellationToken;

/// [`Oncelock`](std::sync::OnceLock::get_or_try_init) is not stabilized yet, so
/// we implement it ourselves. Also we inclose the `Arc`, because why not.
pub struct Lazy<T>(Mutex<Option<Arc<T>>>);

impl<T> Lazy<T> {
    pub const fn new() -> Self {
        Self(Mutex::new(None))
    }

    pub fn get_or_try_init<E>(&self, f: impl FnOnce() -> Result<T, E>) -> Result<Arc<T>, E> {
        let mut guard = self.0.lock().expect("lock poisoned");
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

/// Borrowed or owned bytes >:3
///
/// This type can either be a byte slice `&[u8]` or a `Bytes`.
pub enum Boob<'a> {
    Borrowed(&'a [u8]),
    Owned(Bytes),
}

impl<'a> Boob<'a> {
    pub fn into_owned(self) -> Bytes {
        match self {
            Self::Borrowed(b) => Bytes::copy_from_slice(b),
            Self::Owned(b) => b,
        }
    }
}

impl<'a> AsRef<[u8]> for Boob<'a> {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Borrowed(b) => b,
            Self::Owned(b) => &b,
        }
    }
}

impl<'a> Deref for Boob<'a> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<'a, R: AsRef<[u8]>> PartialEq<R> for Boob<'a> {
    fn eq(&self, other: &R) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl<'a> Hash for Boob<'a> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.deref().hash(state);
    }
}

impl<'a> Debug for Boob<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        self.deref().fmt(f)
    }
}

impl<'a> From<&'a [u8]> for Boob<'a> {
    fn from(value: &'a [u8]) -> Self {
        Self::Borrowed(value)
    }
}

impl From<Bytes> for Boob<'static> {
    fn from(value: Bytes) -> Self {
        Self::Owned(value)
    }
}

#[derive(Debug)]
pub struct UsizeIdGenerator {
    next: usize,
}

impl Default for UsizeIdGenerator {
    fn default() -> Self {
        Self { next: 1 }
    }
}

impl UsizeIdGenerator {
    pub fn next(&mut self) -> NonZeroUsize {
        let id = self.next;
        self.next += 1;
        NonZeroUsize::new(id).expect("id overflow")
    }
}
