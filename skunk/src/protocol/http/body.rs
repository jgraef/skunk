use std::{
    convert::Infallible,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use bytes::{
    Bytes,
    BytesMut,
};
use hyper::body::Frame;
pub use hyper::body::{
    Body,
    Incoming,
};
use pin_project_lite::pin_project;
use tokio::io::{
    AsyncRead,
    ReadBuf,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct Empty;

impl Body for Empty {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        Poll::Ready(None)
    }
}

pin_project! {
    #[derive(Debug)]
    pub struct Read<R> {
        #[pin]
        read: R,
        buf: BytesMut,
        buf_size: usize,
    }
}

impl<R> Read<R> {
    pub fn new(read: R) -> Self {
        Self::with_buf_size(read, 40 * 1024)
    }

    pub fn with_buf_size(read: R, buf_size: usize) -> Self {
        Self {
            read,
            buf: BytesMut::with_capacity(buf_size),
            buf_size,
        }
    }
}

impl<R: AsyncRead> Body for Read<R> {
    type Data = Bytes;
    type Error = std::io::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.project();
        this.buf.resize(*this.buf_size, 0);
        let mut read_buf = ReadBuf::new(this.buf);
        let poll = this.read.poll_read(cx, &mut read_buf);
        let n = read_buf.filled().len();
        poll.map(|result| {
            if let Err(e) = result {
                Some(Err(e))
            }
            else if n == 0 {
                None
            }
            else {
                let buf = this.buf.split_off(n);
                Some(Ok(Frame::data(buf.freeze())))
            }
        })
    }
}
