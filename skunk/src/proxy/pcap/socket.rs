use std::sync::Arc;

use byst::{
    buf::{
        arc_buf::ArcBufMut,
        Slab,
    },
    io::{
        read,
        Read,
        Write,
    },
    Bytes,
};
use parking_lot::Mutex;
use tokio::io::ReadBuf;

use super::interface::Interface;
use crate::protocol::inet::ethernet::MAX_FRAME_SIZE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Raw,
    LinuxSll,
}

/// A "raw" socket. This can be used to send and receive ethernet frames or
/// "cooked" (Linux SLL) packets.[1]
///
/// This can be created with [`Interface::socket`].
///
/// [1]: https://www.man7.org/linux/man-pages/man7/packet.7.html
#[derive(Debug)]
pub struct Socket {
    socket: super::os::Socket,
    interface: Interface,
    mode: Mode,
}

impl Socket {
    pub(super) fn open(interface: Interface, mode: Mode) -> Result<Self, std::io::Error> {
        let socket = super::os::Socket::open(&interface, mode)?;
        Ok(Self {
            socket,
            interface,
            mode,
        })
    }

    /// Receives a packet from the interface.
    ///
    /// The real packet size is returned, even if the buffer wasn't large
    /// enough.
    #[inline]
    pub async fn receive(&self, buf: &mut ReadBuf<'_>) -> Result<usize, std::io::Error> {
        self.socket.receive(buf).await
    }

    #[inline]
    pub async fn send(&self, buf: &[u8]) -> Result<(), std::io::Error> {
        self.socket.send(buf).await
    }

    #[inline]
    pub fn interface(&self) -> &Interface {
        &self.interface
    }

    #[inline]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn into_channel(self) -> (Sender, Receiver) {
        let shared = Arc::new(Shared {
            socket: self,
            slab: Mutex::new(Slab::new(MAX_FRAME_SIZE, 32)),
        });
        let receiver = Receiver {
            shared: shared.clone(),
        };
        let sender = Sender { shared };
        (sender, receiver)
    }
}

#[derive(Debug)]
struct Shared {
    socket: Socket,
    slab: Mutex<Slab>,
}

impl Shared {
    #[inline]
    fn get_buf(&self) -> ArcBufMut {
        let mut slab = self.slab.lock();
        slab.get()
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Receive error")]
pub enum ReceiveError<E> {
    Io(#[from] std::io::Error),

    #[error("Decode error")]
    Decode(#[source] E),
}

/// Listener half used to listen to packets on a socket.
#[derive(Debug)]
pub struct Receiver {
    shared: Arc<Shared>,
}

impl Receiver {
    pub async fn receive<T>(&mut self) -> Result<T, ReceiveError<T::Error>>
    where
        T: Read<Bytes, ()>,
    {
        loop {
            let mut buf = self.shared.get_buf();
            let mut read_buf = unsafe {
                // SAFETY: ReadBuf ensures that no uninitialized values are written to the
                // buffer.
                ReadBuf::uninit(buf.uninitialized_mut())
            };

            let n_read = self.shared.socket.receive(&mut read_buf).await?;

            let initialized = read_buf.initialized().len();
            let filled = read_buf.filled().len();
            unsafe {
                buf.set_initialized_to(initialized);
            }
            buf.set_filled_to(filled);

            if n_read > buf.capacity() {
                tracing::warn!(n_read, buf_capacity = buf.capacity(), "Truncated packet");
            }
            else {
                let mut buf = Bytes::from(buf);
                let packet = read!(&mut buf => T).map_err(ReceiveError::Decode)?;

                break Ok(packet);
            }
        }
    }

    #[inline]
    pub fn socket(&self) -> &Socket {
        &self.shared.socket
    }

    #[inline]
    pub fn interface(&self) -> &Interface {
        self.socket().interface()
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Send error")]
pub enum SendError<E> {
    Io(#[from] std::io::Error),

    #[error("Encode error")]
    Encode(#[source] E),
}

/// Sender half used to send packets on an interface.
///
/// This is cheap to clone.
#[derive(Clone, Debug)]
pub struct Sender {
    shared: Arc<Shared>,
}

impl Sender {
    pub async fn send<T>(&self, _packet: &T) -> Result<(), SendError<()>>
    where
        T: Write<ArcBufMut, ()>,
    {
        //let mut buf = self.shared.get_buf();

        //self.socket.send(buf.filled()).await?;

        //Ok(())

        todo!();
    }

    #[inline]
    pub fn socket(&self) -> &Socket {
        &self.shared.socket
    }

    #[inline]
    pub fn interface(&self) -> &Interface {
        self.socket().interface()
    }
}
