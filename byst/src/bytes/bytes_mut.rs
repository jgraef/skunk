use super::r#impl::BytesMutImpl;

pub struct BytesMut {
    inner: Box<dyn BytesMutImpl>,
}

impl BytesMut {
    #[cfg(feature = "bytes-impl")]
    #[inline]
    pub fn from_impl(inner: Box<dyn BytesMutImpl>) -> Self {
        Self { inner }
    }

    #[cfg(not(feature = "bytes-impl"))]
    #[inline]
    pub(crate) fn from_impl(inner: Box<dyn BytesMutImpl>) -> Self {
        Self { inner }
    }
}

impl BytesMut {
    #[inline]
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self::from_impl(Box::new(Vec::with_capacity(capacity)))
    }
}

impl Default for BytesMut {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

pub struct SpillOver {
    //inner: (),
}
