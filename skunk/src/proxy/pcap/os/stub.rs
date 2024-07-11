use std::{
    io::Error,
    os::fd::{
        AsRawFd,
        RawFd,
    },
};

use tokio::io::ReadBuf;

use crate::proxy::pcap::{
    interface::Interface,
    socket::Mode,
};

fn warn_stub() {
    tracing::warn!("Packet capture is not available on this platform");
}

pub fn list_interfaces() -> Result<Vec<Interface>, Error> {
    warn_stub();
    Ok(vec![])
}

pub fn interface_from_name(_name: &str) -> Result<Option<Interface>, Error> {
    warn_stub();
    Ok(None)
}

#[derive(Debug)]
pub enum Socket {}

impl Socket {
    pub fn open(_interface: &Interface, _mode: Mode) -> Result<Self, Error> {
        unreachable!();
    }

    pub async fn receive(&self, _buf: &mut ReadBuf<'_>) -> Result<usize, Error> {
        unreachable!();
    }

    pub async fn send(&self, _buf: &[u8]) -> Result<(), Error> {
        unreachable!();
    }
}

impl AsRawFd for Socket {
    fn as_raw_fd(&self) -> RawFd {
        unreachable!();
    }
}
