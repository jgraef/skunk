use std::sync::Arc;

use super::{
    chunks::SingleChunk,
    Buf,
};
use crate::{
    bytes::r#impl::{
        BytesImpl,
        ChunksIterImpl,
    },
    Range,
    RangeOutOfBounds,
};

/// A read-only buffer shared using an [`Arc`].
#[derive(Debug)]
pub struct ArcBuf<B> {
    buf: Option<Arc<B>>,
    start: usize,
    end: usize,
}

impl<B> Clone for ArcBuf<B> {
    fn clone(&self) -> Self {
        Self {
            buf: self.buf.clone(),
            start: self.start.clone(),
            end: self.end.clone(),
        }
    }
}

impl<B: AsRef<[u8]>> ArcBuf<B> {
    /// `start <= end` and `start..end` must be in bounds.
    pub fn new(buf: Arc<B>, start: usize, end: usize) -> Self {
        let bytes = (*buf).as_ref();
        assert!(start <= end);
        assert!(end <= bytes.len());
        Self {
            buf: (!bytes.is_empty()).then_some(buf),
            start,
            end,
        }
    }
}

impl<B: AsRef<[u8]> + 'static> Buf for ArcBuf<B> {
    type View<'a> = ArcBuf<B>
    where
        Self: 'a;

    type Chunks<'a> = SingleChunk<'a>
    where
        Self: 'a;

    fn view(&self, range: impl Into<Range>) -> Result<Self::View<'static>, RangeOutOfBounds> {
        let (start, end) = range.into().indices_checked_in(self.start, self.end)?;
        Ok(Self {
            buf: (start < end).then(|| self.buf.clone()).flatten(),
            start,
            end,
        })
    }

    fn chunks(&self, range: impl Into<Range>) -> Result<Self::Chunks<'_>, RangeOutOfBounds> {
        let sub_range: Range = range
            .into()
            .indices_checked_in(self.start, self.end)?
            .into();
        Ok(SingleChunk::new(sub_range.slice_get(
            self.buf.as_deref().map(|x| x.as_ref()).unwrap_or_default(),
        )?))
    }

    fn len(&self) -> usize {
        self.end - self.start
    }
}

impl<B: AsRef<[u8]> + 'static> BytesImpl for ArcBuf<B> {
    fn view(&self, range: Range) -> Result<Box<dyn BytesImpl>, RangeOutOfBounds> {
        let (start, end) = range.indices_checked_in(self.start, self.end)?;
        Ok(Box::new(Self {
            buf: (start < end).then(|| self.buf.clone()).flatten(),
            start,
            end,
        }))
    }

    fn chunks(&self, range: Range) -> Result<Box<dyn ChunksIterImpl<'_> + '_>, RangeOutOfBounds> {
        let sub_range: Range = range.indices_checked_in(self.start, self.end)?.into();
        Ok(Box::new(SingleChunk::new(sub_range.slice_get(
            self.buf.as_deref().map(|x| x.as_ref()).unwrap_or_default(),
        )?)))
    }

    fn len(&self) -> usize {
        self.end - self.start
    }

    fn clone(&self) -> Box<dyn BytesImpl> {
        Box::new(<Self as Clone>::clone(self))
    }
}
