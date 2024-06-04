pub mod ap;
pub mod dhcp;

use std::sync::Arc;

use etherparse::{
    NetSlice,
    SlicedPacket,
    TransportSlice,
};
use futures::TryStreamExt;
use pcap::{
    Capture,
    Device,
    PacketCodec,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("pcap error")]
    Pcap(#[from] pcap::Error),
}

#[derive(Clone, Debug)]
pub struct Interface {
    name: Arc<String>,
}

impl Interface {
    pub fn from_name(name: impl Into<String>) -> Self {
        Self {
            name: Arc::new(name.into()),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

pub fn list_interfaces() -> Result<Vec<Interface>, Error> {
    Ok(Device::list()?
        .into_iter()
        .map(|device| {
            Interface {
                name: Arc::new(device.name),
            }
        })
        .collect())
}

pub async fn run(
    interface: &Interface,
    promisc: bool,
    shutdown: CancellationToken,
) -> Result<(), Error> {
    let capture = Capture::from_device(&**interface.name)?
        .promisc(promisc)
        .open()?
        .setnonblock()?;
    let mut stream = capture.stream(TcpCodec)?;

    tokio::select! {
        _ = shutdown.cancelled() => {},
        result = async move {
            while let Some(_packet) = stream.try_next().await? {}
            Ok::<(), Error>(())
        } => result?,
    }

    Ok(())
}

struct TcpCodec;

impl PacketCodec for TcpCodec {
    type Item = ();

    fn decode(&mut self, packet: pcap::Packet<'_>) -> Self::Item {
        //tracing::debug!(len = packet.data.len(), "packet");

        match SlicedPacket::from_ethernet(&packet.data) {
            Ok(packet) => {
                match packet {
                    SlicedPacket {
                        net: Some(NetSlice::Ipv4(_ipv4)),
                        transport: Some(TransportSlice::Udp(_udp)),
                        ..
                    } => {
                        // todo
                    }
                    _ => {}
                }
            }
            Err(_) => {}
        }

        //println!("{}", pretty_hex(&packet.data));
        ()
    }
}
