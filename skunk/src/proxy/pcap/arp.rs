//! ARP protocol implementation.
//!
//! # References
//! - [An Ethernet Address Resolution Protocol](https://datatracker.ietf.org/doc/html/rfc826)
//! - [Address Resolution Protocol (ARP) Parameters](https://www.iana.org/assignments/arp-parameters/arp-parameters.xhtml)

use std::{
    collections::HashMap,
    fmt::Debug,
    net::IpAddr,
    time::Duration,
};

use futures::TryFutureExt;
use skunk_util::error::ResultExt;
use smallvec::SmallVec;
use tokio::{
    sync::{
        mpsc,
        oneshot,
    },
    time::{
        interval,
        timeout,
    },
};

use super::{
    socket,
    SendError,
};
pub use crate::protocol::inet::arp::Packet;
use crate::protocol::inet::{
    arp::Operation,
    MacAddress,
};

#[derive(Debug)]
pub struct Receiver {
    pub(super) packet_rx: mpsc::Receiver<Packet>,
}

impl Receiver {
    pub async fn receive(&mut self) -> Option<Packet> {
        self.packet_rx.recv().await
    }
}

#[derive(Clone, Debug)]
pub struct Sender {
    pub(super) sock_tx: socket::Sender,
}

impl Sender {
    pub async fn send(&self, _packet: &Packet) -> Result<(), SendError> {
        //self.sock_tx.send(packet).await
        // todo: don't just send the ARP packet! we need to wrap it into an Ethernet
        // frame too lol
        //todo!();
        tracing::debug!("TODO: implement arp::Sender::send");
        Ok(())
    }
}

#[derive(Debug)]
pub struct Socket {
    pub(super) rx: Receiver,
    pub(super) tx: Sender,
}

impl Socket {
    pub async fn receive(&mut self) -> Option<Packet> {
        self.rx.receive().await
    }

    pub async fn send(&self, packet: &Packet) -> Result<(), SendError> {
        self.tx.send(packet).await
    }

    pub fn split(self) -> (Sender, Receiver) {
        (self.tx, self.rx)
    }
}

#[derive(Clone, Debug)]
pub struct Service {
    command_tx: mpsc::Sender<Command>,
}

impl Service {
    pub fn new(socket: Socket) -> Self {
        let (command_tx, command_rx) = mpsc::channel(16);
        Reactor::spawn(socket, command_rx);
        Self { command_tx }
    }

    async fn send_command(&mut self, command: Command) {
        self.command_tx
            .send(command)
            .await
            .expect("ARP service reactor died");
    }

    pub async fn insert(
        &mut self,
        ip_address: IpAddr,
        hardware_address: MacAddress,
        is_self: bool,
    ) {
        self.send_command(Command::Insert {
            ip_address,
            hardware_address,
            is_self,
        })
        .await;
    }

    pub async fn get(
        &mut self,
        sender: (MacAddress, IpAddr),
        ip_address: IpAddr,
    ) -> Option<MacAddress> {
        let (result_tx, result_rx) = oneshot::channel();
        self.send_command(Command::Resolve {
            sender_hardware_address: sender.0,
            sender_ip_address: sender.1,
            ip_address,
            result_tx,
        })
        .await;
        timeout(Duration::from_secs(10), result_rx)
            .map_ok(|r| r.ok())
            .await
            .ok()
            .flatten()
    }
}

#[derive(Debug)]
enum Command {
    Insert {
        ip_address: IpAddr,
        hardware_address: MacAddress,
        is_self: bool,
    },
    Resolve {
        sender_hardware_address: MacAddress,
        sender_ip_address: IpAddr,
        ip_address: IpAddr,
        result_tx: oneshot::Sender<MacAddress>,
    },
}

#[derive(Debug)]
struct Reactor {
    socket: Socket,
    command_rx: mpsc::Receiver<Command>,
    cache: HashMap<IpAddr, CacheEntry>,
    resolving: HashMap<IpAddr, SmallVec<[oneshot::Sender<MacAddress>; 4]>>,
}

impl Reactor {
    fn spawn(socket: Socket, command_rx: mpsc::Receiver<Command>) {
        let reactor = Self {
            socket,
            command_rx,
            cache: HashMap::new(),
            resolving: HashMap::new(),
        };

        tokio::spawn(async move {
            reactor.run().await;
        });
    }

    async fn run(mut self) {
        let mut timeout_interval = interval(Duration::from_secs(10));
        loop {
            tokio::select! {
                command_opt = self.command_rx.recv() => {
                    let Some(command) = command_opt else { break; };
                    self.handle_command(command).await;
                },
                request_opt = self.socket.receive() => {
                    let Some(request) = request_opt else { break; };
                    self.handle_request(request).await;
                }
                _ = timeout_interval.tick() => {
                    // remove any receivers that have been closed.
                    self.resolving.retain(|_, v| {
                        v.retain(|result_tx| {
                            !result_tx.is_closed()
                        });
                        !v.is_empty()
                    });
                }
            }
        }
    }

    async fn handle_command(&mut self, command: Command) {
        match command {
            Command::Insert {
                ip_address,
                hardware_address,
                is_self,
            } => {
                self.cache.insert(
                    ip_address,
                    CacheEntry {
                        hardware_address,
                        is_self,
                    },
                );
            }
            Command::Resolve {
                sender_hardware_address,
                sender_ip_address,
                ip_address,
                result_tx,
            } => {
                if let Some(CacheEntry {
                    hardware_address, ..
                }) = self.cache.get(&ip_address)
                {
                    let _ = result_tx.send(*hardware_address);
                }
                else {
                    let reply = Packet::new(
                        Operation::REQUEST,
                        sender_hardware_address,
                        sender_ip_address,
                        MacAddress::UNSPECIFIED,
                        ip_address,
                    );

                    tracing::debug!(protocol_address = %ip_address, "request");

                    if let Ok(()) = self.socket.send(&reply).await.log_error() {
                        self.resolving
                            .entry(ip_address)
                            .or_default()
                            .push(result_tx);
                    }
                }
            }
        }
    }

    async fn handle_request(&mut self, request: Packet) {
        let mut merge_flag = false;

        if let Some(entry) = self.cache.get_mut(&request.sender_protocol_address) {
            tracing::debug!(protocol_address = %request.sender_protocol_address, hardware_address = %request.sender_hardware_address, "merge");
            entry.hardware_address = request.sender_hardware_address;
            merge_flag = true;
        }

        if let Some(CacheEntry {
            hardware_address,
            is_self: true,
        }) = self.cache.get(&request.target_protocol_address).cloned()
        {
            if !merge_flag {
                tracing::debug!(protocol_address = %request.sender_protocol_address, hardware_address = %request.sender_hardware_address, "new");

                self.cache.insert(
                    request.sender_protocol_address,
                    CacheEntry {
                        hardware_address: request.sender_hardware_address,
                        is_self: false,
                    },
                );

                if let Some(result_txs) = self.resolving.remove(&request.sender_protocol_address) {
                    for result_tx in result_txs {
                        let _ = result_tx.send(request.sender_hardware_address);
                    }
                }
            }

            if request.operation == Operation::REQUEST {
                // note: the RFC states that sender and target get swapped, but doesn't mention
                // that we have to insert our hardware address???
                let reply = Packet::new(
                    Operation::REPLY,
                    hardware_address,
                    request.target_protocol_address,
                    request.sender_hardware_address,
                    request.sender_protocol_address,
                );

                tracing::debug!(protocol_address = %request.target_protocol_address, %hardware_address, "reply");

                let _ = self.socket.send(&reply).await.log_error();
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct CacheEntry {
    hardware_address: MacAddress,
    is_self: bool,
}

fn ip_address_is_same_kind(first: &IpAddr, second: &IpAddr) -> bool {
    matches!(
        (first, second),
        (IpAddr::V4(_), IpAddr::V4(_)) | (IpAddr::V6(_), IpAddr::V6(_))
    )
}

fn assert_ip_address_is_same_kind(first: &IpAddr, second: &IpAddr) {
    if !ip_address_is_same_kind(first, second) {
        panic!("Both IP addresses must be of same version: {first}, {second}");
    }
}
