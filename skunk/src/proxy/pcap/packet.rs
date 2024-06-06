use std::{
    io::Write,
    os::fd::{
        AsRawFd,
        OwnedFd,
    },
    sync::Arc,
};

use etherparse::{
    EtherType,
    Ethernet2Slice,
    Icmpv4Header,
    Icmpv4Slice,
    IpNumber,
    Ipv4Slice,
    PacketBuilderStep,
    TcpHeader,
    TcpSlice,
    TransportSlice,
    UdpHeader,
    UdpSlice,
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

// todo: remove?
#[derive(Debug, thiserror::Error)]
#[error("capture error")]
pub enum Error {
    Io(#[from] std::io::Error),
    Ethernet(#[source] etherparse::err::LenError),
    Ipv4(#[from] etherparse::err::ipv4::SliceError),
    Icmpv4(#[source] etherparse::err::LenError),
    Tcp(#[from] etherparse::err::tcp::HeaderSliceError),
    Udp(#[source] etherparse::err::LenError),
    Encode(#[from] etherparse::err::packet::BuildWriteError),
}

impl From<nix::Error> for Error {
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

    pub async fn next<'a>(&'a mut self) -> Result<LinkPacket<'a>, Error> {
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
) -> Result<Option<LinkPacket<'a>>, Error> {
    // we could also use https://docs.rs/etherparse/latest/etherparse/struct.SlicedPacket.html#method.from_linux_sll

    let n_read = socket.receive(buf).await?;
    let ethernet =
        Ethernet2Slice::from_slice_without_fcs(&buf[..n_read]).map_err(Error::Ethernet)?;

    let network = match ethernet.ether_type() {
        EtherType::ARP => {
            // todo: how do we answer ARP requests?
            NetworkPacket::Arp
        }
        EtherType::IPV4 => {
            let ipv4 = Ipv4Slice::from_slice(ethernet.payload_slice())?;
            let ipv4_payload = ipv4.payload();
            let transport = match ipv4_payload.ip_number {
                IpNumber::ICMP => {
                    let icmpv4 =
                        Icmpv4Slice::from_slice(&ipv4_payload.payload).map_err(Error::Icmpv4)?;
                    TransportSlice::Icmpv4(icmpv4)
                }
                IpNumber::TCP => {
                    let tcp = TcpSlice::from_slice(&ipv4_payload.payload)?;
                    TransportSlice::Tcp(tcp)
                }
                IpNumber::UDP => {
                    let udp = UdpSlice::from_slice(&ipv4_payload.payload).map_err(Error::Udp)?;
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

    Ok(Some(LinkPacket { ethernet, network }))
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

    pub async fn send(&mut self, packet: impl EncodePacket, payload: &[u8]) -> Result<(), Error> {
        packet.encode_packet(payload, &mut self.buf)?;
        self.socket.send(&self.buf).await?;
        self.buf.clear();
        Ok(())
    }
}

pub trait EncodePacket {
    fn encode_packet(self, payload: &[u8], writer: impl Write) -> Result<(), Error>;
}

impl EncodePacket for PacketBuilderStep<Icmpv4Header> {
    fn encode_packet(self, payload: &[u8], mut writer: impl Write) -> Result<(), Error> {
        self.write(&mut writer, payload)?;
        Ok(())
    }
}

impl EncodePacket for PacketBuilderStep<UdpHeader> {
    fn encode_packet(self, payload: &[u8], mut writer: impl Write) -> Result<(), Error> {
        self.write(&mut writer, payload)?;
        Ok(())
    }
}

impl EncodePacket for PacketBuilderStep<TcpHeader> {
    fn encode_packet(self, payload: &[u8], mut writer: impl Write) -> Result<(), Error> {
        self.write(&mut writer, payload)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct LinkPacket<'a> {
    pub ethernet: Ethernet2Slice<'a>,
    pub network: NetworkPacket<'a>,
}

impl<'a> LinkPacket<'a> {
    pub fn source_mac_address(&self) -> MacAddress {
        MacAddress(self.ethernet.source())
    }

    pub fn destination_mac_address(&self) -> MacAddress {
        MacAddress(self.ethernet.destination())
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
