pub mod ap;
//pub mod arp;
//pub mod dhcp;
//pub mod ethernet;
pub mod interface;
pub mod socket;
//pub mod vnet;

use std::convert::Infallible;

use byst::{
    hexdump::hexdump,
    io::ReaderExt,
    Bytes,
};
use socket::ReceiveError;
use tokio_util::sync::CancellationToken;

use self::{
    interface::Interface,
    socket::Mode,
};
use crate::protocol::inet::ethernet;

#[derive(Debug, thiserror::Error)]
#[error("pcap error")]
pub enum Error {
    Io(#[from] std::io::Error),
    InvalidPacket(#[from] ethernet::InvalidFrame<byst::io::End>),
}

impl From<ReceiveError<Infallible>> for Error {
    fn from(value: ReceiveError<Infallible>) -> Self {
        match value {
            ReceiveError::Io(e) => Self::Io(e),
            ReceiveError::Decode(e) => match e {},
        }
    }
}

pub async fn run(interface: &Interface, shutdown: CancellationToken) -> Result<(), Error> {
    let (_sender, mut receiver) = interface.channel(Mode::Raw)?;

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            result = receiver.receive::<Bytes>() => {
                let packet = result?;
                handle_packet(packet).await?;
            }
        }
        todo!();
    }

    Ok(())
}

async fn handle_packet(mut packet: Bytes) -> Result<(), Error> {
    println!("{}", hexdump(&packet));

    let frame: ethernet::Frame = packet.read().unwrap();
    //tracing::debug!(?frame);

    Ok(())
}
