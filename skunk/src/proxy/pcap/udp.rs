use byst::Bytes;
use tokio::sync::mpsc;

use super::{
    socket,
    AddressTriple,
    SendError,
};

#[derive(Clone, Debug)]
pub struct Packet {
    pub source: AddressTriple,
    pub destination: AddressTriple,
    pub data: Bytes,
}

pub struct Sender {
    pub(super) sock_tx: socket::Sender,
}

impl Sender {
    pub async fn send(&mut self) -> Result<(), SendError> {
        tracing::debug!("TODO: implement udp::Sender::send");
        Ok(())
    }
}

pub struct Receiver {
    pub(super) packet_rx: mpsc::Receiver<Packet>,
}

impl Receiver {
    pub async fn receive(&mut self) -> Option<Packet> {
        self.packet_rx.recv().await
    }
}

pub struct Socket {
    pub(super) tx: Sender,
    pub(super) rx: Option<Receiver>,
}

impl Socket {
    pub async fn receive(&mut self) -> Option<Packet> {
        if let Some(rx) = &mut self.rx {
            rx.receive().await
        }
        else {
            None
        }
    }

    pub async fn send(&mut self) -> Result<(), SendError> {
        self.tx.send().await
    }

    pub fn split(self) -> (Sender, Option<Receiver>) {
        (self.tx, self.rx)
    }
}
