use std::{
    io::Write,
    ops::Deref,
    os::fd::{
        AsRawFd,
        OwnedFd,
    },
    sync::Arc,
};

use etherparse::{
    EtherType, Ethernet2Header, Ethernet2Slice, Icmpv4Slice, IpHeaders, IpNumber, Ipv4Slice, PacketBuilderStep, TcpHeader, TcpSlice, TransportSlice, UdpSlice
};
use nix::sys::socket::{
    AddressFamily,
    MsgFlags,
    SockFlag,
    SockProtocol,
    SockType,
};
use tokio::io::{
    unix::AsyncFd,
    Interest,
};

use super::{
    Interface,
    MacAddress,
};
use crate::util::io::WriteBuf;

#[derive(Debug, thiserror::Error)]
#[error("packet receive error")]
pub enum ReceiveError {
    Io(#[from] std::io::Error),
    Encode(#[from] DecodeError),
}

#[derive(Debug, thiserror::Error)]
#[error("packet decode error")]
pub enum DecodeError {
    Ethernet(#[source] etherparse::err::LenError),
    Ipv4(#[from] etherparse::err::ipv4::SliceError),
    Icmpv4(#[source] etherparse::err::LenError),
    Tcp(#[from] etherparse::err::tcp::HeaderSliceError),
    Udp(#[source] etherparse::err::LenError),
}

#[derive(Debug, thiserror::Error)]
#[error("packet send error")]
pub enum SendError {
    Io(#[from] std::io::Error),
    Encode(#[from] EncodeError),
}

#[derive(Debug, thiserror::Error)]
#[error("packet encode error")]
pub enum EncodeError {
    Builder(#[from] etherparse::err::packet::BuildWriteError),

    // this is not a IO error in the sense that something is being sent or received.
    // but when encoding a packet into a buffer the [`Write`] implementation can retun an
    // [`std::io::Error`]
    Write(#[from] std::io::Error),
}

impl From<nix::Error> for SendError {
    fn from(e: nix::Error) -> Self {
        Self::Io(e.into())
    }
}

/// A "raw" socket. This can be used to send and receive ethernet frames.
#[derive(Debug)]
pub struct PacketSocket {
    socket: AsyncFd<OwnedFd>,
}

impl PacketSocket {
    pub fn open(interface: &Interface) -> Result<Self, std::io::Error> {
        // note: the returned OwnedFd will close the socket on drop.
        let socket = nix::sys::socket::socket(
            AddressFamily::Packet,
            SockType::Raw,
            SockFlag::SOCK_NONBLOCK,
            SockProtocol::EthAll,
        )?;

        let bind_address = interface.link_addr();
        // note: we might need to set the protocol field in `bind_addr`, but this is
        // currently not possible. see https://github.com/nix-rust/nix/issues/2059
        nix::sys::socket::bind(socket.as_raw_fd(), bind_address)?;

        let socket = AsyncFd::with_interest(socket, Interest::READABLE | Interest::WRITABLE)?;

        Ok(Self { socket })
    }

    pub async fn receive(&self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        self.socket
            .async_io(Interest::READABLE, |socket| {
                // MsgFlags::MSG_TRUNC tells the kernel to return the real packet length, even
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

    pub fn pair(self) -> (PacketListener, PacketSender) {
        let socket = Arc::new(self);
        (
            PacketListener::new(socket.clone()),
            PacketSender::new(socket),
        )
    }
}

/// Listener half used to listen to packets on a socket.
#[derive(Debug)]
pub struct PacketListener {
    socket: Arc<PacketSocket>,
    buf: Vec<u8>,
}

impl PacketListener {
    const BUF_SIZE: usize = 2048;

    pub fn new(socket: Arc<PacketSocket>) -> Self {
        let buf = vec![0; Self::BUF_SIZE];
        Self { socket, buf }
    }

    pub async fn next<'a>(&'a mut self) -> Result<LinkPacket<'a>, ReceiveError> {
        loop {
            // SAFETY
            // this hack is necessary because rust refuses to understand that we only borrow
            // mutably in *each* interation of the loop and the returned
            // `Packet` contains a shared borrow to `self.buf` that we immediately return
            // (thus not borrowing again). the lifetimes in the method signature
            // make sure that `self` is borrowed as long as the returned
            // `Packet` exists.
            unsafe {
                let buf = &mut *(&mut self.buf as *mut Vec<u8>);
                if let Some(packet) = try_next_packet(&self.socket, buf).await? {
                    return Ok(packet);
                }
            }
        }
    }
}

async fn try_next_packet<'a>(
    socket: &PacketSocket,
    buf: &'a mut [u8],
) -> Result<Option<LinkPacket<'a>>, ReceiveError> {
    // we could also use https://docs.rs/etherparse/latest/etherparse/struct.SlicedPacket.html#method.from_linux_sll

    let n_read = socket.receive(buf).await?;
    let ethernet =
        Ethernet2Slice::from_slice_without_fcs(&buf[..n_read]).map_err(DecodeError::Ethernet)?;

    let network = match ethernet.ether_type() {
        EtherType::ARP => {
            // todo: how do we answer ARP requests?
            NetworkPacket::Arp
        }
        EtherType::IPV4 => {
            let ipv4 =
                Ipv4Slice::from_slice(ethernet.payload_slice()).map_err(DecodeError::from)?;
            let ipv4_payload = ipv4.payload();
            let transport = match ipv4_payload.ip_number {
                IpNumber::ICMP => {
                    let icmpv4 = Icmpv4Slice::from_slice(&ipv4_payload.payload)
                        .map_err(DecodeError::Icmpv4)?;
                    TransportSlice::Icmpv4(icmpv4)
                }
                IpNumber::TCP => {
                    let tcp =
                        TcpSlice::from_slice(&ipv4_payload.payload).map_err(DecodeError::from)?;
                    TransportSlice::Tcp(tcp)
                }
                IpNumber::UDP => {
                    let udp =
                        UdpSlice::from_slice(&ipv4_payload.payload).map_err(DecodeError::Udp)?;
                    TransportSlice::Udp(udp)
                }
                _ => return Ok(None),
            };
            NetworkPacket::Ip { ipv4, transport }
        }
        _ => {
            tracing::debug!(ether_type = ?ethernet.ether_type(), "ignoring packet");
            return Ok(None);
        }
    };

    Ok(Some(LinkPacket {
        ethernet: EthernetHeader(ethernet.to_header()),
        network,
    }))
}

/// Sender half used to send packets on an interface.
///
/// This is cheap to clone.
#[derive(Clone, Debug)]
pub struct PacketSender {
    socket: Arc<PacketSocket>,
    buf: Vec<u8>,
}

impl PacketSender {
    const BUF_SIZE: usize = 2048;

    pub fn new(socket: Arc<PacketSocket>) -> Self {
        let buf = vec![0; Self::BUF_SIZE];
        Self { socket, buf }
    }

    pub async fn send(&mut self, packet: impl WritePacket) -> Result<(), SendError> {
        let mut buf = WriteBuf::new(&mut self.buf);
        packet.write_packet(&mut buf)?;
        self.socket.send(buf.filled()).await?;
        Ok(())
    }
}

pub trait WritePacket {
    fn write_packet(&self, writer: impl Write) -> Result<(), EncodeError>;
}

impl<T: WritePacket> WritePacket for &T {
    fn write_packet(&self, writer: impl Write) -> Result<(), EncodeError> {
        (*self).write_packet(writer)
    }
}

pub struct EthernetFrame<P> {
    pub header: EthernetHeader,
    pub payload: P,
}

impl<P: WritePacket> WritePacket for EthernetFrame<P> {
    fn write_packet(&self, mut writer: impl Write) -> Result<(), EncodeError> {
        self.header.0.write(&mut writer)?;
        self.payload.write_packet(&mut writer)?;
        // todo: don't we have to write the frame check sequence?
        Ok(())
    }
}

pub struct TcpIpPacket<P> {
    pub builder: PacketBuilderStep<TcpHeader>,
    pub payload: P,
}

impl<P: WritePacket> WritePacket for TcpIpPacket<P> {
    fn write_packet(&self, mut writer: impl Write) -> Result<(), EncodeError> {
        // more allocations 🥴
        let mut payload = vec![];
        self.payload.write_packet(WriteBuf::new(&mut payload))?;

        self.builder.clone().write(&mut writer, &payload)?;
        
        // todo: don't we have to write the frame check sequence?
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct LinkPacket<'a> {
    pub ethernet: EthernetHeader,
    pub network: NetworkPacket<'a>,
}

#[derive(Clone, Debug)]
pub struct EthernetHeader(pub Ethernet2Header);

impl From<Ethernet2Header> for EthernetHeader {
    fn from(value: Ethernet2Header) -> Self {
        Self(value)
    }
}

impl EthernetHeader {
    #[inline]
    pub fn source(&self) -> MacAddress {
        self.0.source.into()
    }

    #[inline]
    pub fn destination(&self) -> MacAddress {
        self.0.destination.into()
    }
}

impl Deref for EthernetHeader {
    type Target = Ethernet2Header;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub enum NetworkPacket<'a> {
    Ip {
        ipv4: Ipv4Slice<'a>,
        transport: TransportSlice<'a>,
    },
    Arp,
}
