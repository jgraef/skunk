//! Utilities.

pub(crate) mod boolean;
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
        atomic::{
            AtomicUsize,
            Ordering,
        },
        Arc,
    },
};

pub use bytes;
use bytes::Bytes;
use parking_lot::Mutex;
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
