pub(crate) mod arena;
pub(crate) mod bool_expr;
pub(crate) mod copy;
pub(crate) mod error;

use std::{
    fmt::{
        Debug,
        Formatter,
    },
    hash::{
        Hash,
        Hasher,
    },
    ops::Deref,
    pin::Pin,
    sync::{
        Arc,
        Mutex,
    },
    task::{
        Context,
        Poll,
    },
};

pub use bytes;
use bytes::{
    Buf,
    Bytes,
};
use pin_project_lite::pin_project;
use tokio::io::{
    AsyncRead,
    AsyncWrite,
    ReadBuf,
};
pub use tokio_util::sync::CancellationToken;

/// [`Oncelock`](std::sync::OnceLock::get_or_try_init) is not stabilized yet, so
/// we implement it ourselves. Also we inclose the `Arc`, because why not.
pub struct Lazy<T>(Mutex<Option<Arc<T>>>);

impl<T> Lazy<T> {
    pub const fn new() -> Self {
        Self(Mutex::new(None))
    }

    pub fn get_or_try_init<E>(&self, f: impl FnOnce() -> Result<T, E>) -> Result<Arc<T>, E> {
        let mut guard = self.0.lock().expect("lock poisoned");
        if let Some(value) = &*guard {
            Ok(value.clone())
        }
        else {
            let value = Arc::new(f()?);
            *guard = Some(value.clone());
            Ok(value)
        }
    }
}

pin_project! {
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

/// Borrowed or owned bytes :3
pub enum Boob<'a> {
    Borrowed(&'a [u8]),
    Owned(Bytes),
}

impl<'a> Boob<'a> {
    pub fn into_owned(self) -> Bytes {
        match self {
            Self::Borrowed(b) => Bytes::copy_from_slice(b),
            Self::Owned(b) => b,
        }
    }
}

impl<'a> AsRef<[u8]> for Boob<'a> {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Borrowed(b) => b,
            Self::Owned(b) => &b,
        }
    }
}

impl<'a> Deref for Boob<'a> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<'a, R: AsRef<[u8]>> PartialEq<R> for Boob<'a> {
    fn eq(&self, other: &R) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl<'a> Hash for Boob<'a> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.deref().hash(state);
    }
}

impl<'a> Debug for Boob<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        self.deref().fmt(f)
    }
}

pin_project! {
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

    pub fn was_shutdown(&self) -> bool {
        self.shutdown
    }

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
