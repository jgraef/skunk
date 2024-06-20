use std::{
    os::fd::{
        AsRawFd,
        OwnedFd,
    },
    sync::Arc,
};

use byst::{
    buf::{
        arc_buf::ArcBufMut,
        Slab,
    },
    io::{
        read,
        write::Write,
        Cursor,
        Read,
    },
    Bytes,
};
use nix::sys::socket::{
    AddressFamily,
    MsgFlags,
    SockFlag,
    SockProtocol,
    SockType,
};
use parking_lot::Mutex;
use tokio::io::{
    unix::AsyncFd,
    Interest,
};

use super::interface::Interface;
use crate::protocol::inet::ethernet::MAX_FRAME_SIZE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Raw,
    LinuxSll,
}

impl Mode {
    fn as_sock_type(&self) -> SockType {
        match self {
            Self::Raw => SockType::Raw,
            Self::LinuxSll => SockType::Datagram,
        }
    }
}

/// A "raw" socket. This can be used to send and receive ethernet frames.[1]
///
/// This can be created with [`Interface::socket`].
///
/// [1]: https://www.man7.org/linux/man-pages/man7/packet.7.html
#[derive(Debug)]
pub struct Socket {
    socket: AsyncFd<OwnedFd>,
    interface: Interface,
    mode: Mode,
}

impl Socket {
    pub(super) fn open(interface: Interface, mode: Mode) -> Result<Self, std::io::Error> {
        // note: the returned OwnedFd will close the socket on drop.
        let socket = nix::sys::socket::socket(
            AddressFamily::Packet,
            mode.as_sock_type(),
            SockFlag::SOCK_NONBLOCK,
            SockProtocol::EthAll,
        )?;

        let bind_address = interface
            .raw_bind_address()
            .expect("interface has no address we can bind to");
        // note: we might need to set the protocol field in `bind_addr`, but this is
        // currently not possible. see https://github.com/nix-rust/nix/issues/2059
        nix::sys::socket::bind(socket.as_raw_fd(), &bind_address)?;

        let socket = AsyncFd::with_interest(socket, Interest::READABLE | Interest::WRITABLE)?;

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
    pub async fn receive(&self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        self.socket
            .async_io(Interest::READABLE, |socket| {
                // `MsgFlags::MSG_TRUNC` tells the kernel to return the real packet length, even
                // if the buffer is not large enough.
                Ok(nix::sys::socket::recv(
                    socket.as_raw_fd(),
                    buf,
                    MsgFlags::MSG_TRUNC,
                )?)
            })
            .await
    }

    pub async fn send(&self, buf: &[u8]) -> Result<(), std::io::Error> {
        let bytes_sent = self
            .socket
            .async_io(Interest::WRITABLE, |socket| {
                Ok(nix::sys::socket::send(
                    socket.as_raw_fd(),
                    buf,
                    MsgFlags::empty(),
                )?)
            })
            .await?;
        if bytes_sent < buf.len() {
            tracing::warn!(buf_len = buf.len(), bytes_sent, "sent truncated packet");
        }
        Ok(())
    }

    #[inline]
    pub fn interface(&self) -> &Interface {
        &self.interface
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
        let mut buf = slab.get();
        buf.fully_initialize();
        buf
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
        T: Read<Cursor<Bytes>, ()>,
    {
        loop {
            let mut buf = self.shared.get_buf();

            let n_read = self.shared.socket.receive(buf.initialized_mut()).await?;

            if n_read > buf.capacity() {
                tracing::warn!(n_read, buf_capacity = buf.capacity(), "Truncated packet");
            }
            else {
                buf.set_filled_to(n_read);
                let buf = Bytes::from(buf);
                let mut cursor = Cursor::new(buf);
                let packet = read!(&mut cursor => T).map_err(ReceiveError::Decode)?;
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
    pub async fn send<T>(&mut self, _packet: T) -> Result<(), SendError<()>>
    where
        T: Write<Cursor<ArcBufMut>>,
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
