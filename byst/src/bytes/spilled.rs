#![allow(unused_variables, dead_code)]

use std::sync::Arc;

use super::r#impl::{
    BytesMutImpl,
    BytesMutViewImpl,
    BytesMutViewMutImpl,
    ChunksMutIterImpl,
};
use crate::{
    buf::{
        chunks::WithOffset,
        Full,
        Length,
        SizeLimit,
    },
    Range,
    RangeOutOfBounds,
};

struct Segment {
    start: usize,
    end: usize,
    buf: Box<dyn BytesMutImpl>,
}

struct Spilled {
    inner: Arc<Vec<Segment>>,
}

impl FromIterator<Box<dyn BytesMutImpl>> for Spilled {
    #[inline]
    fn from_iter<T: IntoIterator<Item = Box<dyn BytesMutImpl>>>(iter: T) -> Self {
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

impl Length for Spilled {
    fn len(&self) -> usize {
        self.inner.last().map(|last| last.end).unwrap_or_default()
    }
}

impl BytesMutImpl for Spilled {
    fn view(&self, range: Range) -> Result<Box<dyn BytesMutViewImpl + '_>, RangeOutOfBounds> {
        todo!()
    }

    fn view_mut(
        &mut self,
        range: Range,
    ) -> Result<Box<dyn BytesMutViewMutImpl + '_>, RangeOutOfBounds> {
        todo!()
    }

    fn chunks(
        &self,
        range: Range,
    ) -> Result<Box<dyn super::r#impl::ChunksIterImpl<'_> + '_>, RangeOutOfBounds> {
        todo!()
    }

    fn chunks_mut(
        &mut self,
        range: Range,
    ) -> Result<Box<dyn ChunksMutIterImpl<'_> + '_>, RangeOutOfBounds> {
        todo!()
    }

    fn reserve(&mut self, size: usize) -> Result<(), Full> {
        todo!()
    }

    fn grow(&mut self, new_len: usize, value: u8) -> Result<(), Full> {
        todo!()
    }

    fn extend(&mut self, with: &[u8]) -> Result<(), Full> {
        todo!()
    }

    fn size_limit(&self) -> SizeLimit {
        SizeLimit::Unlimited
    }

    fn split_at(
        self,
        at: usize,
    ) -> Result<(Box<dyn BytesMutImpl>, Box<dyn BytesMutImpl>), crate::IndexOutOfBounds> {
        todo!()
    }
}
