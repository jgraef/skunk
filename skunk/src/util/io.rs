//! IO utilities.

use std::{
    net::Ipv4Addr,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use byteorder::ByteOrder;
use bytes::{
    Buf,
    BufMut,
    Bytes,
    BytesMut,
};
use pin_project_lite::pin_project;
use tokio::io::{
    AsyncRead,
    AsyncReadExt,
    AsyncWrite,
    ReadBuf,
};

use crate::proxy::pcap::MacAddress;

pin_project! {
    /// Wrapper for [`AsyncRead`]/[`AsyncWrite`] types that "rewinds" a read operation.
    /// This is done by giving it the bytes that you already read, but want to put back.
    /// [`Rewind`] will return these buffered bytes first when read is called on it.
    ///
    /// This also implements [`AsyncWrite`] as we often want to use connections bidirectionally,
    /// but it doesn't have any effect on writes.
    #[derive(Debug)]
    pub struct Rewind<T> {
        #[pin]
        inner: T,
        buf: Bytes,
    }
}

impl<T> Rewind<T> {
    pub fn new(inner: T, buf: Bytes) -> Self {
        Self { inner, buf }
    }

    /// Returns the underlying IO stream and the buffer containing data that
    /// wasn't read yet.
    pub fn into_parts(self) -> (T, Bytes) {
        (self.inner, self.buf)
    }
}

impl<T: AsyncRead> AsyncRead for Rewind<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let this = self.project();
        if this.buf.remaining() == 0 {
            this.inner.poll_read(cx, buf)
        }
        else {
            let n = std::cmp::min(this.buf.len(), buf.remaining());
            buf.put_slice(&this.buf[..n]);
            this.buf.advance(n);
            if this.buf.remaining() == 0 {
                // make sure the underlying buffer can be deallocated
                *this.buf = Bytes::from_static(b"");
            }
            Poll::Ready(Ok(()))
        }
    }
}

impl<T: AsyncWrite> AsyncWrite for Rewind<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        self.project().inner.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        self.project().inner.poll_shutdown(cx)
    }
}

pin_project! {
    /// Wrapper for [`AsyncRead`]/[`AsyncWrite`] streams that ignores any shutdowns issued by the consumer.
    ///
    /// This will just ignore calls to [`AsyncWrite::poll_shutdown`].
    #[derive(Debug)]
    pub struct WithoutShutdown<T> {
        #[pin]
        inner: T,
        shutdown: bool,
    }
}

impl<T> WithoutShutdown<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            shutdown: false,
        }
    }

    /// Returns whether [`Self::poll_shutdown`] was called on this stream.
    pub fn was_shutdown(&self) -> bool {
        self.shutdown
    }

    /// Returns the wrapped stream.
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: AsyncRead> AsyncRead for WithoutShutdown<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        self.project().inner.poll_read(cx, buf)
    }
}

impl<T: AsyncWrite> AsyncWrite for WithoutShutdown<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        self.project().inner.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        *self.project().shutdown = true;
        Poll::Ready(Ok(()))
    }
}

pin_project! {
    /// IO stream that is either of two variants.
    #[project = EitherProj]
    pub enum EitherStream<L, R> {
        Left { #[pin] inner: L },
        Right { #[pin] inner: R },
    }
}

impl<L, R> AsyncRead for EitherStream<L, R>
where
    L: AsyncRead,
    R: AsyncRead,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.project() {
            EitherProj::Left { inner } => inner.poll_read(cx, buf),
            EitherProj::Right { inner } => inner.poll_read(cx, buf),
        }
    }
}

impl<L, R> AsyncWrite for EitherStream<L, R>
where
    L: AsyncWrite,
    R: AsyncWrite,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        match self.project() {
            EitherProj::Left { inner } => inner.poll_write(cx, buf),
            EitherProj::Right { inner } => inner.poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        match self.project() {
            EitherProj::Left { inner } => inner.poll_flush(cx),
            EitherProj::Right { inner } => inner.poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        match self.project() {
            EitherProj::Left { inner } => inner.poll_shutdown(cx),
            EitherProj::Right { inner } => inner.poll_shutdown(cx),
        }
    }
}

pub async fn read_nul_terminated<S>(mut socket: S) -> Result<BytesMut, std::io::Error>
where
    S: AsyncRead + Unpin,
{
    let mut buf = BytesMut::new();
    loop {
        let b = socket.read_u8().await?;
        if b == 0 {
            break;
        }
        buf.put_u8(b);
    }
    Ok(buf)
}

pub struct SliceReader<'a>(&'a [u8]);

macro_rules! read_impl {
    ($name:ident, $n:expr, $out:ty) => {
        #[inline]
        pub fn $name<B: ByteOrder>(&mut self) -> Result<$out, std::io::Error> {
            if self.0.len() >= $n {
                let v = B::$name(&self.0[..$n]);
                self.0 = &self.0[$n..];
                Ok(v)
            }
            else {
                Err(std::io::ErrorKind::UnexpectedEof.into())
            }
        }
    };
}

impl<'a> SliceReader<'a> {
    #[inline]
    pub fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }

    #[inline]
    pub fn read_u8(&mut self) -> Result<u8, std::io::Error> {
        if self.0.len() >= 1 {
            let b = self.0[0];
            self.0 = &self.0[1..];
            Ok(b)
        }
        else {
            Err(std::io::ErrorKind::UnexpectedEof.into())
        }
    }

    #[inline]
    pub fn read_i8(&mut self) -> Result<i8, std::io::Error> {
        Ok(self.read_u8()? as i8)
    }

    read_impl!(read_u16, 2, u16);
    read_impl!(read_i16, 2, i16);
    read_impl!(read_u32, 4, u32);
    read_impl!(read_i32, 4, i32);
    read_impl!(read_u64, 8, u64);
    read_impl!(read_i64, 8, i64);
    read_impl!(read_u128, 16, u128);
    read_impl!(read_i128, 16, i128);

    pub fn read_subslice(&mut self, n: impl Into<usize>) -> Result<&'a [u8], std::io::Error> {
        let n = n.into();
        if self.0.len() >= n {
            let (sub, rest) = self.0.split_at(n);
            self.0 = rest;
            Ok(sub)
        }
        else {
            Err(std::io::ErrorKind::UnexpectedEof.into())
        }
    }

    #[inline]
    pub fn sub_reader(&mut self, n: impl Into<usize>) -> Result<SliceReader, std::io::Error> {
        Ok(SliceReader(self.read_subslice(n)?))
    }

    #[inline]
    fn read_mac_address(&mut self) -> Result<MacAddress, std::io::Error> {
        if self.0.len() >= 6 {
            let v = MacAddress::from([
                self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5],
            ]);
            self.0 = &self.0[6..];
            Ok(v)
        }
        else {
            Err(std::io::ErrorKind::UnexpectedEof.into())
        }
    }

    #[inline]
    fn read_ipv4_address(&mut self) -> Result<Ipv4Addr, std::io::Error> {
        if self.0.len() >= 4 {
            let v = Ipv4Addr::new(self.0[0], self.0[1], self.0[2], self.0[3]);
            self.0 = &self.0[6..];
            Ok(v)
        }
        else {
            Err(std::io::ErrorKind::UnexpectedEof.into())
        }
    }

    pub fn skip(&mut self, n: usize) -> Result<(), std::io::Error> {
        if self.0.len() >= n {
            self.0 = &self.0[n..];
            Ok(())
        }
        else {
            Err(std::io::ErrorKind::UnexpectedEof.into())
        }
    }

    pub fn rest(self) -> &'a [u8] {
        self.0
    }
}
