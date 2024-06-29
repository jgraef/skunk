use super::{
    read::ReadError,
    BufReader,
    End,
    Reader,
    Seek,
};
use crate::buf::{
    BufMut,
    Length,
};

#[derive(Clone, Debug)]
pub struct Count<R> {
    inner: R,
    count: usize,
}

impl<R> Count<R> {
    #[inline]
    pub fn new(inner: R) -> Self {
        Self { inner, count: 0 }
    }

    #[inline]
    pub fn count(&self) -> usize {
        self.count
    }

    #[inline]
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R> From<R> for Count<R> {
    #[inline]
    fn from(value: R) -> Self {
        Self::new(value)
    }
}

impl<R: Reader> Reader for Count<R> {
    type Error = <R as Reader>::Error;

    fn read_into<D: BufMut>(
        &mut self,
        dest: D,
        limit: impl Into<Option<usize>>,
    ) -> Result<usize, Self::Error> {
        match self.inner.read_into(dest, limit) {
            Ok(n_read) => {
                self.count += n_read;
                Ok(n_read)
            }
            Err(e) => {
                self.count += e.amount_read();
                Err(e)
            }
        }
    }

    fn read_into_exact<D: BufMut>(&mut self, dest: D, length: usize) -> Result<(), Self::Error> {
        match self.inner.read_into_exact(dest, length) {
            Ok(()) => {
                self.count += length;
                Ok(())
            }
            Err(e) => {
                self.count += e.amount_read();
                Err(e)
            }
        }
    }

    fn skip(&mut self, amount: usize) -> Result<(), Self::Error> {
        match Reader::skip(&mut self.inner, amount) {
            Ok(()) => {
                self.count += amount;
                Ok(())
            }
            Err(e) => {
                self.count += e.amount_read();
                Err(e)
            }
        }
    }
}

impl<R: BufReader> BufReader for Count<R> {
    type View = R::View;

    #[inline]
    fn peek_chunk(&self) -> Option<&[u8]> {
        self.inner.peek_chunk()
    }

    #[inline]
    fn view(&mut self, length: usize) -> Result<Self::View, End> {
        let view = self.inner.view(length)?;
        self.count += view.len();
        Ok(view)
    }

    #[inline]
    fn peek_view(&self, length: usize) -> Result<Self::View, End> {
        self.inner.peek_view(length)
    }

    #[inline]
    fn rest(&mut self) -> Self::View {
        let view = self.inner.rest();
        self.count += view.len();
        view
    }

    #[inline]
    fn peek_rest(&self) -> Self::View {
        self.inner.peek_rest()
    }

    #[inline]
    fn advance(&mut self, by: usize) -> Result<(), End> {
        self.inner.advance(by)?;
        self.count += by;
        Ok(())
    }

    #[inline]
    fn remaining(&self) -> usize {
        self.inner.remaining()
    }
}

impl<R: Seek> Seek for Count<R> {
    type Position = Count<R::Position>;

    #[inline]
    fn tell(&self) -> Self::Position {
        Count {
            inner: self.inner.tell(),
            count: self.count,
        }
    }

    #[inline]
    fn seek(&mut self, position: &Self::Position) -> Self::Position {
        let position = Count {
            inner: self.inner.seek(&position.inner),
            count: self.count,
        };
        self.count = position.count;
        position
    }
}
