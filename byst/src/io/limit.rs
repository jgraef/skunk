use super::{
    BufReader,
    End,
    Reader,
};
use crate::BufMut;

pub struct Limit<R> {
    inner: R,
    limit: usize,
}

impl<R> Limit<R> {
    #[inline]
    pub fn new(inner: R, limit: usize) -> Self {
        Self { inner, limit }
    }

    #[inline]
    pub fn remaining_limit(&self) -> usize {
        self.limit
    }

    #[inline]
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: Reader> Limit<R> {
    #[inline]
    pub fn skip_remaining(&mut self) -> Result<(), End> {
        let skipped = self.inner.skip(self.limit);
        self.limit -= skipped;
        if self.limit == 0 {
            Ok(())
        }
        else {
            Err(End)
        }
    }
}

impl<R: Reader> Reader for Limit<R> {
    #[inline]
    fn read_into<D: BufMut>(&mut self, dest: D, limit: impl Into<Option<usize>>) -> usize {
        let limit = if let Some(limit) = limit.into() {
            std::cmp::min(self.limit, limit)
        }
        else {
            self.limit
        };

        let n_read = self.inner.read_into(dest, limit);

        self.limit -= n_read;

        n_read
    }

    #[inline]
    fn skip(&mut self, amount: usize) -> usize {
        if amount > self.limit {
            0
        }
        else {
            let skipped = Reader::skip(&mut self.inner, amount);
            self.limit -= skipped;
            skipped
        }
    }
}

impl<R: BufReader> BufReader for Limit<R> {
    type View = R::View;

    #[inline]
    fn view(&self, length: usize) -> Result<Self::View, End> {
        if length > self.limit {
            Err(End)
        }
        else {
            self.inner.view(length)
        }
    }

    #[inline]
    fn chunk(&self) -> Result<&[u8], End> {
        if self.limit == 0 {
            Err(End)
        }
        else {
            let chunk = self.inner.chunk()?;
            Ok(&chunk[..std::cmp::min(chunk.len(), self.limit)])
        }
    }

    #[inline]
    fn advance(&mut self, by: usize) -> Result<(), End> {
        if by > self.limit {
            Err(End)
        }
        else {
            self.inner.advance(by)?;
            self.limit -= by;
            Ok(())
        }
    }

    #[inline]
    fn remaining(&self) -> usize {
        std::cmp::min(self.limit, self.inner.remaining())
    }

    #[inline]
    fn rest(&mut self) -> Self::View {
        match self.inner.view(self.limit) {
            Ok(view) => {
                self.limit = 0;
                view
            }
            Err(End) => self.inner.rest(),
        }
    }
}
