pub mod ap;
pub mod arp;
//pub mod dhcp;
pub mod interface;
mod os;
pub mod socket;
pub mod udp;

use std::{
    collections::HashMap,
    convert::Infallible,
    net::{
        IpAddr,
        Ipv4Addr,
    },
    ops::RangeInclusive,
};

use byst::{
    io::{
        End,
        ReaderExt,
    },
    Buf,
    Bytes,
};
use ip_network::Ipv4Network;
use skunk_macros::{
    ipv4_address,
    ipv4_network,
};
use tokio::sync::mpsc;
use tracing::Instrument;

use self::{
    interface::Interface,
    socket::{
        Mode,
        ReceiveError,
    },
};
use crate::{
    protocol::inet::{
        ethernet,
        ipv4,
        MacAddress,
    },
    util::error::ResultExt,
};

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
        let dhcp_server = ipv4_address!("10.0.69.1");
        Self {
            subnet: ipv4_network!("10.0.69.0/24"),
            dhcp_server,
            router: dhcp_server,
            dns_servers: vec![dhcp_server],
            pool: ipv4_address!("10.0.69.100")..=ipv4_address!("10.0.69.200"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("pcap error")]
pub enum Error {
    Io(#[from] std::io::Error),
    InvalidPacket(
        #[from]
        ethernet::InvalidFrame<
            End,
            ethernet::AnyPayloadError<End, ipv4::AnyPayloadError<Infallible>>,
        >,
    ),
}

impl From<ReceiveError<Infallible>> for Error {
    fn from(value: ReceiveError<Infallible>) -> Self {
        match value {
            ReceiveError::Io(e) => Self::Io(e),
            ReceiveError::Decode(e) => match e {},
        }
    }
}

#[derive(Clone, Debug)]
pub struct VirtualNetwork {
    command_tx: mpsc::Sender<Command>,
    sock_tx: socket::Sender,
    interface: Interface,
    arp: arp::Service,
}

impl VirtualNetwork {
    pub fn new(interface: &Interface) -> Result<Self, Error> {
        let (command_tx, command_rx) = mpsc::channel(16);
        let (sock_tx, sock_rx) = interface.channel(Mode::Raw)?;

        let (arp_tx, arp_rx) = mpsc::channel(16);
        let arp = arp::Service::new(arp::Socket {
            rx: arp::Receiver { packet_rx: arp_rx },
            tx: arp::Sender {
                sock_tx: sock_tx.clone(),
            },
        });

        let reactor = Reactor {
            interface: interface.clone(),
            sock_tx: sock_tx.clone(),
            sock_rx,
            command_rx,
            arp_listener: arp_tx,
            udp_listeners: HashMap::new(),
        };

        reactor.spawn();

        Ok(Self {
            command_tx,
            sock_tx,
            interface: interface.clone(),
            arp,
        })
    }

    async fn send_command(&self, command: Command) {
        self.command_tx.send(command).await.expect("Reactor died");
    }

    pub async fn host(&mut self, hardware_address: MacAddress, ip_address: IpAddr) -> VirtualHost {
        self.arp.insert(ip_address, hardware_address, true).await;
        VirtualHost {
            network: self.clone(),
            hardware_address,
            ip_address,
        }
    }
}

#[derive(Debug)]
struct Reactor {
    interface: Interface,
    sock_tx: socket::Sender,
    sock_rx: socket::Receiver,
    command_rx: mpsc::Receiver<Command>,
    arp_listener: mpsc::Sender<arp::Packet>,
    udp_listeners: HashMap<(IpAddr, u16), mpsc::Sender<udp::Packet>>,
}

impl Reactor {
    fn spawn(self) {
        let span = tracing::info_span!("reactor");
        tokio::spawn(
            async move {
                tracing::debug!("reactor spawned");
                let _ = self.run().await.log_error();
                tracing::debug!("reactor done");
            }
            .instrument(span),
        );
    }

    async fn run(mut self) -> Result<(), Error> {
        loop {
            tokio::select! {
                command_opt = self.command_rx.recv() => {
                    if let Some(command) = command_opt {
                        self.handle_command(command).await?;
                    }
                    else {
                        // All instances of `VirtualNetwork` have been dropped, so we terminate.
                        break;
                    }
                },
                packet_res = self.sock_rx.receive::<Bytes>() => {
                    let packet = packet_res?;
                    let _ = self.handle_packet(packet).await.log_error();
                }
            }
        }

        Ok(())
    }

    async fn handle_packet(&mut self, packet: Bytes) -> Result<(), Error> {
        //tracing::debug!("{}", hexdump(&packet));

        let frame: EthernetFrame = packet.reader().read()?;
        tracing::debug!("Ethernet: {:#?}", frame.header);

        match frame.payload {
            ethernet::AnyPayload::Arp(arp_packet) => {
                tracing::debug!("ARP: {:#?}", arp_packet);
                self.arp_listener
                    .send(arp_packet)
                    .await
                    .expect("ARP service died");
            }
            ethernet::AnyPayload::Ipv4(ip_packet) => {
                tracing::debug!("IPv4: {:#?}", ip_packet.header);

                match ip_packet.payload {
                    ipv4::AnyPayload::Udp(udp_packet) => {
                        tracing::debug!("UDP: {:#?}", udp_packet.header);
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        Ok(())
    }

    async fn handle_command(&mut self, command: Command) -> Result<(), Error> {
        match command {
            Command::RegisterUdpListener {
                bind_address,
                packet_tx,
            } => {
                self.udp_listeners.insert(bind_address, packet_tx);
            }
        }
        Ok(())
    }
}

type EthernetFrame = ethernet::Frame<ethernet::AnyPayload<ipv4::AnyPayload>>;

#[derive(Clone, Copy, Debug)]
pub struct AddressTriple {
    pub mac_address: MacAddress,
    pub ip_address: IpAddr,
    pub port: u16,
}

#[derive(Debug)]
enum Command {
    RegisterUdpListener {
        bind_address: (IpAddr, u16),
        packet_tx: mpsc::Sender<udp::Packet>,
    },
}

#[derive(Clone, Debug)]
pub struct VirtualHost {
    network: VirtualNetwork,
    hardware_address: MacAddress,
    ip_address: IpAddr,
}

impl VirtualHost {
    pub async fn udp_socket(&self, bind_address: Option<(IpAddr, u16)>) -> udp::Socket {
        let rx = if let Some(bind_address) = bind_address {
            let (packet_tx, packet_rx) = mpsc::channel(16);
            self.network
                .send_command(Command::RegisterUdpListener {
                    bind_address,
                    packet_tx,
                })
                .await;
            Some(udp::Receiver { packet_rx })
        }
        else {
            None
        };

        udp::Socket {
            rx,
            tx: udp::Sender {
                sock_tx: self.network.sock_tx.clone(),
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Send error")]
pub struct SendError;
