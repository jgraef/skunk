pub mod ap;
pub mod arp;
pub mod dhcp;
pub mod ethernet;
pub mod interface;
pub mod packet;
pub mod vnet;

use std::fmt::Debug;

use etherparse::TransportSlice;
use tokio_util::sync::CancellationToken;

pub use self::interface::Interface;
use self::packet::NetworkPacket;
pub use crate::protocol::inet::MacAddress;

// todo: remove?
#[derive(Debug, thiserror::Error)]
#[error("pcap error")]
pub enum Error {
    Io(#[from] std::io::Error),
    Send(#[from] self::packet::SendError),
    Receive(#[from] self::packet::ReceiveError),
    Dhcp(#[from] self::dhcp::Error),
    Arp(#[from] self::arp::Error),
}

impl From<nix::Error> for Error {
    fn from(e: nix::Error) -> Self {
        Self::Io(e.into())
    }
}

pub async fn run(interface: Interface, shutdown: CancellationToken) -> Result<(), Error> {
    let (mut reader, _sender) = interface.socket()?.into_pair();

    loop {
        let packet = tokio::select! {
            result = reader.next() => result?,
            _ = shutdown.cancelled() => break,
        };

        println!("ethernet:");
        println!("  source:      {}", packet.ethernet.source());
        println!("  destination: {}", packet.ethernet.destination());

        if let NetworkPacket::Ip { ipv4, transport } = packet.network {
            let ipv4_header = ipv4.header();

            println!("ipv4:");
            println!("  source:      {}", ipv4_header.source_addr());
            println!("  destination: {}", ipv4_header.destination_addr());

            match transport {
                TransportSlice::Udp(udp) => {
                    println!("udp:");
                    println!("  source:      {}", udp.source_port());
                    println!("  destination: {}", udp.destination_port());
                    println!("  payload:     {} bytes", udp.payload().len());
                }
                TransportSlice::Tcp(tcp) => {
                    println!("tcp:");
                    println!("  source:      {}", tcp.source_port());
                    println!("  destination: {}", tcp.destination_port());
                    println!("  payload:     {} bytes", tcp.payload().len());
                }
                _ => {}
            }
        }

        println!();
    }

    Ok(())
}
