use std::{
    fmt::Display,
    net::Ipv4Addr,
};

use dhcproto::{
    Decodable,
    Encodable,
};
use etherparse::TransportSlice;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::{
    dhcp::{
        self,
        NetworkConfig,
    },
    packet::{
        LinkPacket,
        NetworkPacket,
        PacketListener,
        PacketSender,
        PacketSocket,
    },
};

#[derive(Debug, thiserror::Error)]
#[error("virtual network error")]
pub enum Error {
    Packet(#[from] super::packet::Error),
    Dhcp(#[from] super::dhcp::Error),
}

#[derive(Debug)]
pub struct VirtualNetwork {
    sender: PacketSender,
    shutdown: CancellationToken,
    join_handle: JoinHandle<Result<(), Error>>,
}

impl VirtualNetwork {
    pub fn new(socket: PacketSocket, network_config: NetworkConfig) -> Self {
        let (listener, sender) = socket.pair();
        let shutdown = CancellationToken::new();

        let dhcp = dhcp::Service::new(network_config);

        let join_handle = tokio::spawn(reactor(listener, sender.clone(), dhcp, shutdown.clone()));

        Self {
            sender,
            shutdown,
            join_handle,
        }
    }

    pub async fn shutdown(self) -> Result<(), Error> {
        self.shutdown.cancel();
        self.join_handle.await.ok().transpose()?;
        Ok(())
    }
}

async fn reactor(
    mut listener: PacketListener,
    sender: PacketSender,
    mut dhcp: dhcp::Service,
    shutdown: CancellationToken,
) -> Result<(), Error> {
    loop {
        tokio::select! {
            // shutdown requested
            _ = shutdown.cancelled() => break,

            // read a packet from the interface
            //
            // not sure if this is cancellation-safe. our next impl should be, but `AsyncFd`'s cancellation behaviour is not documented.
            // according to Alice Ryhl in the tokio Discord it's safe "unless the closure does something weird".
            result = listener.next() => {
                let packet = result?;
                handle_packet(&packet, &sender, &mut dhcp).await?;
            }
        }
    }

    Ok(())
}

async fn handle_packet<'a>(
    packet: &'a LinkPacket<'a>,
    sender: &PacketSender,
    dhcp: &mut dhcp::Service,
) -> Result<(), Error> {
    const BUF_SIZE: usize = 2048;
    let mut buf = vec![0; BUF_SIZE];

    match &packet.network {
        NetworkPacket::Ip {
            ipv4,
            transport: TransportSlice::Udp(udp),
        } => {
            let ipv4_header = ipv4.header();

            // handle DHCP messages
            if udp.destination_port() == dhcp::SERVER_PORT
                && udp.source_port() == dhcp::CLIENT_PORT
                && is_receiver_for_ipv4(
                    ipv4_header.destination_addr(),
                    dhcp.network_config().dhcp_server,
                    dhcp.network_config().subnet.broadcast_address(),
                )
            {
                let request =
                    dhcp::Message::from_bytes(udp.payload()).map_err(dhcp::Error::from)?;

                dhcp.handle_message(
                    request,
                    DhcpSender {
                        sender,
                        buf: &mut buf,
                    },
                )
                .await?;
            }
            else {
                tracing::debug!(
                    source = %DisplayIpPort(ipv4_header.source_addr(), udp.source_port()),
                    destination = %DisplayIpPort(ipv4_header.destination_addr(), udp.destination_port()),
                    "udp packet ignored"
                );
                // todo: for now we drop all other UDP packets.
            }
        }
        NetworkPacket::Ip {
            ipv4,
            transport: TransportSlice::Tcp(tcp),
        } => {
            let ipv4_header = ipv4.header();

            tracing::debug!(
                source = %DisplayIpPort(ipv4_header.source_addr(), tcp.source_port()),
                destination = %DisplayIpPort(ipv4_header.destination_addr(), tcp.destination_port()),
                "tcp packet ignored"
            );
        }
        NetworkPacket::Arp => {
            // todo
        }
        _ => {}
    }

    Ok(())
}

struct DhcpSender<'a> {
    sender: &'a PacketSender,
    buf: &'a mut Vec<u8>,
}

impl<'a> dhcp::Sender for DhcpSender<'a> {
    async fn send(
        &mut self,
        response: &dhcp::Message,
        _to: dhcp::SendTo,
    ) -> Result<(), dhcp::Error> {
        let mut encoder = dhcproto::Encoder::new(self.buf);
        response.encode(&mut encoder)?;

        todo!();
        //Ok(())
    }
}

fn is_receiver_for_ipv4(
    destination: Ipv4Addr,
    receiver: Ipv4Addr,
    subnet_broadcast: Ipv4Addr,
) -> bool {
    destination.is_broadcast() || destination == subnet_broadcast || destination == receiver
}

pub struct DisplayIpPort(pub Ipv4Addr, pub u16);

impl Display for DisplayIpPort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.0, self.1)
    }
}
