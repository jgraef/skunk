#![allow(unused_variables, dead_code)]

use std::sync::Arc;

use super::r#impl::BytesMutImpl;
use crate::buf::{
    chunks::WithOffset,
    Length,
};

struct Segment<'b> {
    start: usize,
    end: usize,
    buf: Box<dyn BytesMutImpl<'b>>,
}

struct Spilled<'b> {
    inner: Arc<Vec<Segment<'b>>>,
}

impl<'b> FromIterator<Box<dyn BytesMutImpl<'b>>> for Spilled<'b> {
    #[inline]
    fn from_iter<T: IntoIterator<Item = Box<dyn BytesMutImpl<'b>>>>(iter: T) -> Self {
        Self {
            inner: Arc::new(
                WithOffset::new(iter.into_iter())
                    .map(|(offset, buf)| {
                        Segment {
                            start: offset,
                            end: offset + buf.len(),
                            buf,
                        }
                    })
                    .collect(),
            ),
        }
    }
}

impl<'b> Length for Spilled<'b> {
    fn len(&self) -> usize {
        self.inner.last().map(|last| last.end).unwrap_or_default()
    }
}
