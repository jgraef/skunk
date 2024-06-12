//! IO utilities.

use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

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

pub struct WriteBuf<B> {
    buf: B,
    offset: usize,
}

impl<B> WriteBuf<B> {
    pub fn new(buf: B) -> Self {
        Self { buf, offset: 0 }
    }

    pub fn filled_amount(&self) -> usize {
        self.offset
    }

    pub fn into_inner(self) -> B {
        self.buf
    }
}

impl<B: AsRef<[u8]>> WriteBuf<B> {
    pub fn filled(&self) -> &[u8] {
        &self.buf.as_ref()[..self.offset]
    }
}

macro_rules! impl_write_buf_for_slice {
    ($ty:ty; $($generics:tt)*) => {
        impl<$($generics)*> std::io::Write for WriteBuf<$ty> {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                let remaining = self.buf.len() - self.offset;
                let write_amount = std::cmp::min(remaining, buf.len());
                if write_amount == 0 {
                    return Err(std::io::ErrorKind::UnexpectedEof.into());
                }
                self.buf[self.offset..][..write_amount].copy_from_slice(&buf[..write_amount]);
                Ok(write_amount)
            }

            fn flush(&mut self) -> std::io::Result<()> {
                // nop
                Ok(())
            }
        }
    };
}

impl_write_buf_for_slice!(&mut [u8];);
impl_write_buf_for_slice!([u8; N]; const N: usize);
impl_write_buf_for_slice!(Box<[u8]>;);

macro_rules! impl_write_buf_for_vec {
    ($ty:ty) => {
        impl std::io::Write for WriteBuf<$ty> {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                let new_size = self.offset + buf.len();
                self.buf.resize(new_size, 0);
                self.buf[self.offset..][..buf.len()].copy_from_slice(&buf[..buf.len()]);
                Ok(buf.len())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                // nop
                Ok(())
            }
        }
    };
}

impl_write_buf_for_vec!(&mut Vec<u8>);
impl_write_buf_for_vec!(Vec<u8>);
