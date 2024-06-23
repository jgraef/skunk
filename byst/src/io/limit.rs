use std::convert::Infallible;

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

impl<R: Reader> Limit<R>
where
    <R as Reader>::Error: FailedPartially,
{
    pub fn skip_remaining(&mut self) -> Result<(), <R as Reader>::Error> {
        match Reader::skip(&mut self.inner, self.limit) {
            Ok(()) => {
                self.limit = 0;
                Ok(())
            }
            Err(e) => {
                self.limit -= e.partial_amount();
                Err(e)
            }
        }
    }
}

impl<R: Reader> Reader for Limit<R>
where
    <R as Reader>::Error: FailedPartially,
{
    type Error = LimitError<<R as Reader>::Error>;

    fn read_into<D: BufMut>(
        &mut self,
        dest: D,
        limit: impl Into<Option<usize>>,
    ) -> Result<usize, Self::Error> {
        let limit = if let Some(limit) = limit.into() {
            std::cmp::min(self.limit, limit)
        }
        else {
            self.limit
        };

        match self.inner.read_into(dest, limit) {
            Ok(n_read) => {
                self.limit -= n_read;
                Ok(n_read)
            }
            Err(e) => {
                self.limit -= e.partial_amount();
                Err(LimitError::Inner(e))
            }
        }
    }

    fn read_into_exact<D: BufMut>(&mut self, dest: D, length: usize) -> Result<(), Self::Error> {
        if length > self.limit {
            Err(LimitError::LimitReached)
        }
        else {
            match self.inner.read_into_exact(dest, length) {
                Ok(()) => {
                    self.limit -= length;
                    Ok(())
                }
                Err(e) => {
                    self.limit -= e.partial_amount();
                    Err(LimitError::Inner(e))
                }
            }
        }
    }

    fn skip(&mut self, amount: usize) -> Result<(), Self::Error> {
        let amount = std::cmp::min(self.limit, amount);

        match Reader::skip(&mut self.inner, amount) {
            Ok(()) => {
                self.limit -= amount;
                Ok(())
            }
            Err(e) => {
                self.limit -= e.partial_amount();
                Err(LimitError::Inner(e))
            }
        }
    }
}

impl<R: BufReader> BufReader for Limit<R>
where
    <R as Reader>::Error: FailedPartially,
{
    type View = R::View;

    #[inline]
    fn view(&self, length: usize) -> Result<Self::View, End> {
        if length > self.limit {
            Err(End {
                read: 0,
                requested: length,
                remaining: self.limit.min(self.inner.remaining()),
            })
        }
        else {
            self.inner.view(length)
        }
    }

    #[inline]
    fn chunk(&self) -> Option<&[u8]> {
        if self.limit == 0 {
            None
        }
        else {
            let chunk = self.inner.chunk()?;
            Some(&chunk[..std::cmp::min(chunk.len(), self.limit)])
        }
    }

    #[inline]
    fn advance(&mut self, by: usize) -> Result<(), End> {
        if by > self.limit {
            Err(End {
                read: 0,
                requested: by,
                remaining: self.limit.min(self.inner.remaining()),
            })
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
            Err(_) => self.inner.rest(),
        }
    }
}

#[diagnostic::on_unimplemented(
    message = "The error type `{Self}` doesn't provide information about partial failures.",
    note = "For `Limit` to work the error type of the inner reader or writer needs to provide information about partial reads or writes. Otherwise the `Limit` can't know how to update it's internal counter.",
    note = "Implement `FailedPartially` for {Self}"
)]
pub trait FailedPartially {
    fn partial_amount(&self) -> usize;
}

impl FailedPartially for Infallible {
    fn partial_amount(&self) -> usize {
        match *self {}
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LimitError<E> {
    #[error("Inner reader failed")]
    Inner(#[source] E),

    #[error("Limit reached")]
    LimitReached,
}

impl<E: FailedPartially> FailedPartially for LimitError<E> {
    fn partial_amount(&self) -> usize {
        match self {
            Self::Inner(e) => e.partial_amount(),
            Self::LimitReached => {
                // when we reach the limit for an exact read, we don't do a partial read.
                0
            }
        }
    }
}
