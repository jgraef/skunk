use std::{
    future::Future,
    mem::MaybeUninit,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use pin_project_lite::pin_project;
use tokio::io::{
    AsyncRead,
    AsyncWrite,
    ReadBuf,
};

struct CopyBuf {
    buf: Vec<MaybeUninit<u8>>,
    filled: usize,
    initialized: usize,
    written: usize,
    read_closed: bool,
}

impl CopyBuf {
    pub fn new(buf_size: usize) -> Self {
        let mut buf = Vec::with_capacity(buf_size);
        buf.resize(buf_size, MaybeUninit::uninit());
        Self {
            buf,
            filled: 0,
            initialized: 0,
            written: 0,
            read_closed: false,
        }
    }

    pub fn poll_copy<I: AsyncRead, O: AsyncWrite>(
        &mut self,
        cx: &mut Context<'_>,
        mut in_stream: Pin<&mut I>,
        mut out_stream: Pin<&mut O>,
    ) -> Poll<Result<(), std::io::Error>> {
        let mut buf = ReadBuf::uninit(&mut self.buf);
        unsafe { buf.assume_init(self.initialized) };
        buf.advance(self.filled);

        loop {
            if self.written == 0 && !self.read_closed {
                // read

                match in_stream.as_mut().poll_read(cx, &mut buf) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                    Poll::Ready(Ok(())) => {
                        // the read was successful. continue with writing.
                        self.filled = buf.filled().len();
                        self.initialized = buf.initialized().len();
                        self.read_closed = self.filled == 0;
                    }
                }
            }

            // this condition is only false, when self.written == self.filled == 0
            if self.written < self.filled {
                // write

                match out_stream
                    .as_mut()
                    .poll_write(cx, &buf.filled()[self.written..])
                {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                    Poll::Ready(Ok(num_written)) => {
                        // the write was successful, continue with reading (if everything was
                        // written) or writing.
                        self.written += num_written;
                        assert!(self.written <= self.filled);
                        if self.written == self.filled {
                            self.written = 0;
                            self.filled = 0;
                            buf.clear();
                        }
                    }
                }
            }
            else {
                // nothing was read, and we wrote everything, so we flush and then we're done.

                assert!(self.read_closed);
                return out_stream.as_mut().poll_flush(cx);
            }
        }
    }
}

pin_project! {
    pub struct CopyBidirectional<S, T> {
        #[pin]
        source: S,
        #[pin]
        target: T,
        in_copy: Option<CopyBuf>,
        out_copy: Option<CopyBuf>,
    }
}

impl<S, T> CopyBidirectional<S, T> {
    pub fn new(source: S, target: T) -> Self {
        Self::with_buf_sizes(source, target, 4096, 4096)
    }

    pub fn with_buf_sizes(source: S, target: T, in_buf_size: usize, out_buf_size: usize) -> Self {
        Self {
            source,
            target,
            in_copy: Some(CopyBuf::new(in_buf_size)),
            out_copy: Some(CopyBuf::new(out_buf_size)),
        }
    }
}

impl<S, T> Future for CopyBidirectional<S, T>
where
    S: AsyncRead + AsyncWrite,
    T: AsyncRead + AsyncWrite,
{
    type Output = Result<(), std::io::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        if let Some(in_copy) = &mut this.in_copy {
            match in_copy.poll_copy(cx, Pin::new(&mut this.target), Pin::new(&mut this.source)) {
                Poll::Pending => {}
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Ready(Ok(())) => {
                    *this.in_copy = None;
                }
            }
        }

        if let Some(out_copy) = &mut this.out_copy {
            match out_copy.poll_copy(cx, Pin::new(&mut this.source), Pin::new(&mut this.target)) {
                Poll::Pending => {}
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Ready(Ok(())) => {
                    *this.out_copy = None;
                }
            }
        }

        if this.in_copy.is_none() && this.out_copy.is_none() {
            Poll::Ready(Ok(()))
        }
        else {
            Poll::Pending
        }
    }
}
