use std::{
    fmt::Display,
    net::Ipv4Addr,
    ops::RangeInclusive,
};

use etherparse::TransportSlice;
use ip_network::Ipv4Network;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::{
    dhcp,
    interface::Socket,
    packet::{
        LinkPacket,
        NetworkPacket,
        PacketListener,
        PacketSender,
    },
    Error,
    MacAddress,
};

#[derive(Debug)]
pub struct VirtualNetwork {
    sender: PacketSender,
    shutdown: CancellationToken,
    join_handle: JoinHandle<Result<(), Error>>,
}

impl VirtualNetwork {
    pub fn new(socket: Socket, network_config: NetworkConfig) -> Self {
        let (listener, sender) = socket.into_pair();
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
    mut sender: PacketSender,
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
                handle_packet(&packet, &mut sender, &mut dhcp).await?;
            }
        }
    }

    Ok(())
}

async fn handle_packet<'a>(
    packet: &'a LinkPacket<'a>,
    sender: &mut PacketSender,
    dhcp: &mut dhcp::Service,
) -> Result<(), Error> {
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
                let request = dhcp::Packet::from_bytes(udp.payload()).map_err(dhcp::Error::from)?;

                let network_config = dhcp.network_config();
                let from = (
                    sender.interface().hardware_address(),
                    network_config.dhcp_server,
                    dhcp::SERVER_PORT,
                );

                dhcp.handle_message(
                    &request,
                    DhcpSender {
                        sender,
                        from,
                        to_hardware_address: packet.ethernet.source(),
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
        NetworkPacket::Arp => if packet.ethernet.destination().is_broadcast() {},
        _ => {}
    }

    Ok(())
}

#[derive(Debug)]
struct DhcpSender<'a> {
    sender: &'a mut PacketSender,
    from: (MacAddress, Ipv4Addr, u16),
    to_hardware_address: MacAddress,
}

impl<'a> dhcp::Sender for DhcpSender<'a> {
    async fn send(
        &mut self,
        _response: &dhcp::Packet,
        to: dhcp::SendTo,
    ) -> Result<(), super::packet::SendError> {
        let _destination = to.to_socket_address();

        /*UdpIpPacket {
            builder: PacketBuilder::ethernet2(self.from.0.into(), self.to_hardware_address)
                .ipv4(self.from.1.into(), destination.0, 64),
            payload: todo!(),
        }*/
        todo!();
    }
}

/*
#[derive(Debug)]
struct ArpSender<'a> {
    sender: &'a mut PacketSender,
    source: MacAddress,
    destination: MacAddress,
}

impl<'a> arp::Sender for ArpSender<'a> {
    async fn send<H: arp::HardwareAddress, P: arp::ProtocolAddress>(
        &mut self,
        response: &arp::ArpPacket<H, P>,
    ) -> Result<(), super::packet::SendError> {
        self.sender
            .send(EthernetFrame {
                header: Ethernet2Header {
                    source: self.source.into(),
                    destination: self.destination.into(),
                    ether_type: EtherType::ARP,
                }
                .into(),
                payload: response,
            })
            .await?;

        Ok(())
    }
}
 */

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

#[derive(Debug)]
pub struct NetworkConfig {
    pub subnet: Ipv4Network,
    pub dhcp_server: Ipv4Addr,
    pub router: Ipv4Addr,
    pub dns_servers: Vec<Ipv4Addr>,
    pub pool: RangeInclusive<Ipv4Addr>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        let dhcp_server = Ipv4Addr::new(10, 0, 69, 1);
        Self {
            subnet: Ipv4Network::new(Ipv4Addr::new(10, 0, 69, 0), 24).unwrap(),
            dhcp_server,
            router: dhcp_server,
            dns_servers: vec![dhcp_server],
            pool: Ipv4Addr::new(10, 0, 69, 100)..=Ipv4Addr::new(10, 0, 69, 200),
        }
    }
}
