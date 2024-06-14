use std::{
    fmt::Debug,
    ops::Deref,
    sync::Arc,
};

use super::{
    buf::chunks::SingleChunk,
    Buf,
    Range,
    RangeOutOfBounds,
};

/// Type alias for [`Bytes`] that are `'static`.
pub type Sbytes = Bytes<'static>;

#[derive(Clone)]
enum Inner<'a> {
    Static {
        buf: &'static [u8],
    },
    Borrowed {
        buf: &'a [u8],
    },
    Shared {
        buf: Arc<[u8]>,
        start: usize,
        end: usize,
    },
}

impl<'a> Inner<'a> {
    #[inline]
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Static { buf } => buf,
            Self::Borrowed { buf } => buf,
            Self::Shared { buf, start, end } => &buf[*start..*end],
        }
    }
}

/// A cheap-to-clone contiguous immutable buffer. This internally has 3 variants
/// to achieve this:
///
/// 1. The underlying buffer is a `'static` slice.
/// 2. The underlying buffer is borrowed immutably.
/// 3. The underlying buffer is inside an [`Arc`].
///
/// [`Bytes`] are created by:
///
/// - Using [`Bytes::new`] to create an empty buffer.
/// - Using various [`From`] implementations.
/// - Using [`Bytes::from_static`] to create a statically-shared buffer.
///
/// The [`Bytes`] struct has only a few methods in its inherent `impl`. Most
/// functionality comes from it implementing [`Buf`] and
/// [`Deref<Target=u8>`](Deref).
///
/// If you have a `Bytes<'a>` you can turn it into a `Bytes<'static>` by calling
/// [`Bytes::as_static`]. This will only copy data if the buffer was borrowed
/// (variant 2).
///
/// If you think writing `Bytes<'static>` all the time is too much writing,
/// there is a type alias [`Sbytes`] for it :3
///
/// # TODO
///
/// Some of the conversions (e.g. from `Box<[u8]>` or `Vec<u8>`) allocate. We
/// could create create an enum for them, put that into `Inner::Shared` and
/// also store a pointer for faster access (after pinning it).
#[derive(Clone)]
pub struct Bytes<'a> {
    inner: Inner<'a>,
}

impl Bytes<'static> {
    /// Creates a statically shared buffer. Use this instead of [`From::from`]
    /// if you know the lifetime of your slice is `'static`, otherwise the
    /// implementation has no way of knowing that the slice is actually
    /// `'static` and will clone the data when calling [`Bytes::as_static`].
    /// Otherwise there is no difference.
    #[inline]
    pub fn from_static(bytes: &'static [u8]) -> Self {
        Self {
            inner: Inner::Static { buf: bytes },
        }
    }
}

impl<'a> Bytes<'a> {
    /// Creates an empty [`Bytes`].
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a [`Bytes`] with a `'static` lifetime.
    ///
    /// This only copies the data, if the original bytes were borrowed.
    #[inline]
    pub fn as_static(&self) -> Bytes<'static> {
        match &self.inner {
            Inner::Static { buf } => Bytes::from_static(buf),
            Inner::Borrowed { buf } => {
                Bytes {
                    inner: Inner::Shared {
                        buf: Arc::from(*buf),
                        start: 0,
                        end: buf.len(),
                    },
                }
            }
            Inner::Shared { buf, start, end } => {
                Bytes {
                    inner: Inner::Shared {
                        buf: buf.clone(),
                        start: *start,
                        end: *end,
                    },
                }
            }
        }
    }

    /// If `subset` is a slice contained in the [`Bytes`], this returns a view
    /// for that slice.
    ///
    /// This is useful if you're using some function that only returns a
    /// sub-slice `&[u8]` from a [`Bytes`], but you want to have that sub-slice
    /// as a view.
    pub fn view_from_slice(&self, subset: &[u8]) -> Option<Self> {
        if subset.is_empty() {
            Some(Self::default())
        }
        else {
            let bytes_ptr = self.inner.bytes().as_ptr() as usize;
            let bytes_len = self.inner.bytes().len();
            let sub_ptr = subset.as_ptr() as usize;
            let sub_len = subset.len();

            (sub_ptr >= bytes_ptr && sub_ptr + sub_len <= bytes_ptr + bytes_len).then(|| {
                let sub_offset = sub_ptr - bytes_ptr;
                self.view(sub_offset..(sub_offset + sub_len)).unwrap()
            })
        }
    }
}

impl<'a> Default for Bytes<'a> {
    #[inline]
    fn default() -> Self {
        Bytes::from_static(b"")
    }
}

impl<'a> Debug for Bytes<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.inner.bytes().iter()).finish()
    }
}

impl<'a> AsRef<[u8]> for Bytes<'a> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.inner.bytes()
    }
}

impl<'a> Deref for Bytes<'a> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.inner.bytes()
    }
}

impl<'a> From<&'a [u8]> for Bytes<'a> {
    #[inline]
    fn from(value: &'a [u8]) -> Self {
        Self {
            inner: Inner::Borrowed { buf: value },
        }
    }
}

impl<'a, const N: usize> From<&'a [u8; N]> for Bytes<'a> {
    #[inline]
    fn from(value: &'a [u8; N]) -> Self {
        (value as &[u8]).into()
    }
}

impl<const N: usize> From<[u8; N]> for Bytes<'static> {
    #[inline]
    fn from(value: [u8; N]) -> Self {
        Arc::<[u8]>::from(value).into()
    }
}

impl From<Arc<[u8]>> for Bytes<'static> {
    #[inline]
    fn from(value: Arc<[u8]>) -> Self {
        let end = value.len();
        Self {
            inner: Inner::Shared {
                buf: value,
                start: 0,
                end,
            },
        }
    }
}

impl From<Box<[u8]>> for Bytes<'static> {
    #[inline]
    fn from(value: Box<[u8]>) -> Self {
        Arc::<[u8]>::from(value).into()
    }
}

impl From<Vec<u8>> for Bytes<'static> {
    #[inline]
    fn from(value: Vec<u8>) -> Self {
        Arc::<[u8]>::from(value).into()
    }
}

impl<'a, const N: usize> TryFrom<Bytes<'a>> for [u8; N] {
    type Error = std::array::TryFromSliceError;

    #[inline]
    fn try_from(value: Bytes<'a>) -> Result<Self, Self::Error> {
        (*value).try_into()
    }
}

impl<'b> Buf for Bytes<'b> {
    type View<'a> = Self where Self: 'a;

    type Chunks<'a> = SingleChunk<'a> where Self: 'a;

    fn view(&self, range: impl Into<Range>) -> Result<Self, RangeOutOfBounds> {
        let range = range.into();
        match &self.inner {
            Inner::Static { buf } => {
                Ok(Self {
                    inner: Inner::Borrowed {
                        buf: range.slice_get(buf)?,
                    },
                })
            }
            Inner::Borrowed { buf } => {
                Ok(Self {
                    inner: Inner::Borrowed {
                        buf: range.slice_get(buf)?,
                    },
                })
            }
            Inner::Shared { buf, start, end } => {
                let (start, end) = range.indices_checked_in(*start, *end)?;
                if start == end {
                    Ok(Self::default())
                }
                else {
                    Ok(Self {
                        inner: Inner::Shared {
                            buf: buf.clone(),
                            start,
                            end,
                        },
                    })
                }
            }
        }
    }

    #[inline]
    fn chunks(&self, range: impl Into<Range>) -> Result<Self::Chunks<'_>, RangeOutOfBounds> {
        Ok(SingleChunk::new(
            range.into().slice_get(self.inner.bytes())?,
        ))
    }

    #[inline]
    fn len(&self) -> usize {
        self.inner.bytes().len()
    }
}
