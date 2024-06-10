//! Zero-copy reading

use std::ops::RangeBounds;

use bytes::Bytes;

use super::bytes_wip::endianness::{
    Endianness,
    Size,
};

macro_rules! default_read_int_impls {
    {$($out:ty => $name:ident;)*} => {
        $(
            #[inline]
            fn $name<E: Endianness>(&mut self) -> Result<$out, std::io::Error>
            where
                [(); <$out as Size>::BYTES]: Sized,
            {
                //Ok(<$out as Decode<E>>::decode(&self.read_array()?))
                todo!();
            }
        )*
    };
}

pub trait Reader: Sized {
    type Slice: AsRef<[u8]>;

    fn read_slice(&mut self, n: impl Into<usize>) -> Result<Self::Slice, std::io::Error>;
    fn skip_reader(&mut self, n: impl Into<usize>) -> Result<Self, std::io::Error>;
    fn rest_reader(&mut self) -> Self;
    fn rest(self) -> Self::Slice;
    fn offset(&self) -> usize;
    fn remaining(&self) -> usize;

    #[inline]
    fn skip(&mut self, n: impl Into<usize>) -> Result<(), std::io::Error> {
        self.skip_reader(n)?;
        Ok(())
    }

    #[inline]
    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], std::io::Error> {
        Ok(self.read_slice(N)?.as_ref().try_into().unwrap())
    }

    #[inline]
    fn read_u8(&mut self) -> Result<u8, std::io::Error> {
        Ok(self.read_array::<1>()?[0])
    }

    #[inline]
    fn read_i8(&mut self) -> Result<i8, std::io::Error> {
        Ok(self.read_u8()? as i8)
    }

    default_read_int_impls! {
        u16 => read_u16;
        i16 => read_i16;
        u32 => read_u32;
        i32 => read_i32;
        u64 => read_u64;
        i64 => read_i64;
        u128 => read_u128;
        i128 => read_i128;
    }
}

pub trait Read {}

#[derive(Clone)]
pub struct Cursor<B> {
    buf: B,
    offset: usize,
}

impl<B> Cursor<B> {
    #[inline]
    pub fn new(buf: B) -> Self {
        Self { buf, offset: 0 }
    }

    pub fn into_inner(self) -> B {
        self.buf
    }
}

impl<B: AsRef<[u8]>> Cursor<B> {
    fn advance(&mut self, n: impl Into<usize>) -> Result<(usize, usize), std::io::Error> {
        let offset = self.offset;
        let n = n.into();
        if self.offset + n < self.buf.as_ref().len() {
            self.offset += n;
            Ok((offset, self.offset))
        }
        else {
            Err(std::io::ErrorKind::UnexpectedEof.into())
        }
    }
}

macro_rules! impl_cursor {
    (($($generics:tt)*), $buf:ty, $slice:expr) => {
        impl<$($generics)*> Reader for Cursor<$buf> {
            type Slice = $buf;

            #[inline]
            fn read_slice(&mut self, n: impl Into<usize>) -> Result<Self::Slice, std::io::Error> {
                let (start, end) = self.advance(n)?;
                Ok($slice(&self.buf, start..end))
            }

            #[inline]
            fn skip_reader(&mut self, n: impl Into<usize>) -> Result<Self, std::io::Error> {
                let (start, end) = self.advance(n)?;
                Ok(Self {
                    buf: $slice(&self.buf, start..end),
                    offset: 0,
                })
            }

            #[inline]
            fn rest_reader(&mut self) -> Self {
                Self {
                    buf: $slice(&self.buf, self.offset..),
                    offset: 0,
                }
            }

            #[inline]
            fn rest(self) -> Self::Slice {
                $slice(&self.buf, self.offset..)
            }

            #[inline]
            fn offset(&self) -> usize {
                self.offset
            }

            #[inline]
            fn remaining(&self) -> usize {
                self.buf.len() - self.offset
            }
        }

    };
}

#[inline]
fn slice_slice(buf: &[u8], range: impl RangeBounds<usize>) -> &[u8] {
    &buf[(range.start_bound().cloned(), range.end_bound().cloned())]
}

#[inline]
fn slice_bytes(buf: &Bytes, range: impl RangeBounds<usize>) -> Bytes {
    buf.slice(range)
}

impl_cursor!(('a), &'a [u8], slice_slice);
impl_cursor!((), Bytes, slice_bytes);
