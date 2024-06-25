use super::{
    read::ReadError,
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
    pub fn skip_remaining(&mut self) -> Result<(), <R as Reader>::Error> {
        match self.inner.skip(self.limit) {
            Ok(()) => {
                self.limit = 0;
                Ok(())
            }
            Err(e) => {
                self.limit -= e.amount_read();
                Err(e)
            }
        }
    }
}

impl<R: Reader> Reader for Limit<R> {
    type Error = <R as Reader>::Error;

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
                self.limit -= e.amount_read();
                Err(e)
            }
        }
    }

    fn read_into_exact<D: BufMut>(&mut self, dest: D, length: usize) -> Result<(), Self::Error> {
        if length > self.limit {
            Err(Self::Error::from_end(End {
                read: 0,
                requested: length,
                remaining: self.limit,
            }))
        }
        else {
            match self.inner.read_into_exact(dest, length) {
                Ok(()) => {
                    self.limit -= length;
                    Ok(())
                }
                Err(e) => {
                    self.limit -= e.amount_read();
                    Err(e)
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
                self.limit -= e.amount_read();
                Err(e)
            }
        }
    }
}

impl<R: BufReader> BufReader for Limit<R> {
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
