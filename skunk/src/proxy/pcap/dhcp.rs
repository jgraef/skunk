use std::{
    collections::{
        BTreeSet,
        HashMap,
    },
    fmt::{
        Debug,
        Display,
    },
    net::Ipv4Addr,
    ops::RangeInclusive,
};

use bytes::Bytes;
use dhcproto::{
    v4::{
        DhcpOption,
        HType,
        Message,
        MessageType,
        Opcode,
        OptionCode,
        CLIENT_PORT,
        SERVER_PORT,
    },
    Decodable,
    Encodable,
    Encoder,
};
use indexmap::{
    map::Entry,
    IndexMap,
};
use ip_network::Ipv4Network;
use tokio::{
    io::ReadBuf,
    net::UdpSocket,
};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use super::Interface;

const BUF_SIZE: usize = 1024;
const LEASE_TIME: u32 = 24 * 60 * 60;

#[derive(Debug, thiserror::Error)]
#[error("dhcp error")]
pub enum Error {
    Io(#[from] std::io::Error),
    Decode(#[from] dhcproto::error::DecodeError),
    Encode(#[from] dhcproto::error::EncodeError),
    InvalidRequest(#[from] InvalidRequest),
}

#[derive(Debug, thiserror::Error)]
#[error("invalid request")]
pub enum InvalidRequest {
    #[error("DHCPREQUEST without IP address")]
    NoRequestedIpAddress,
}

pub async fn run(
    interface: &Interface,
    shutdown: CancellationToken,
    network_config: NetworkConfig,
) -> Result<(), Error> {
    let span = tracing::info_span!("dhcpd");
    tracing::debug!(parent: &span, port = %SERVER_PORT, "starting DHCP server");

    tokio::select! {
        result = async move {
            Server::new(interface, network_config).await?
                .serve().await?;
            Ok::<(), Error>(())
        }.instrument(span) => result?,
        _ = shutdown.cancelled() => {}
    };

    Ok(())
}

macro_rules! get_opt {
    ($message:expr, $opt:ident) => {{
        if let Some(DhcpOption::$opt(value)) = $message.opts().get(OptionCode::$opt) {
            Some(value)
        }
        else {
            None
        }
    }};
}

pub struct Server {
    socket: UdpSocket,
    buf: Vec<u8>,
    network_config: NetworkConfig,
    leases: Leases,
}

impl Server {
    pub async fn new(interface: &Interface, network_config: NetworkConfig) -> Result<Self, Error> {
        let socket = UdpSocket::bind((Ipv4Addr::BROADCAST, SERVER_PORT)).await?;
        socket.bind_device(Some(interface.name.as_bytes()))?;
        socket.set_broadcast(true)?;

        let buf = Vec::with_capacity(BUF_SIZE);

        let leases = Leases::new(network_config.pool.clone());

        Ok(Self {
            socket,
            buf,
            network_config,
            leases,
        })
    }

    pub async fn serve(mut self) -> Result<(), Error> {
        loop {
            self.buf.resize(BUF_SIZE, 0);
            let mut read_buf = ReadBuf::new(&mut self.buf);

            let (_, client_address) = self.socket.recv_buf_from(&mut read_buf).await?;
            if client_address.port() != CLIENT_PORT {
                tracing::warn!(
                    port = client_address.port(),
                    "received message from unusual port"
                );
                continue;
            }

            let message = Message::from_bytes(read_buf.filled())?;
            self.buf.clear();

            self.handle_message(message).await?;
        }
    }

    async fn handle_message(&mut self, message: Message) -> Result<(), Error> {
        if message.opcode() != Opcode::BootRequest
            || message.htype() != HType::Eth
            || message.hlen() != 6
        {
            tracing::warn!("received invalid dhcp request");
            return Ok(());
        }

        tracing::debug!(message = ?MessageDebug(&message), "received");

        if let Some(message_type) = get_opt!(message, MessageType) {
            match message_type {
                MessageType::Discover => {
                    self.handle_discover(message).await?;
                }
                MessageType::Request => {
                    self.handle_request(message).await?;
                }
                _ => {
                    tracing::debug!(r#type = ?message_type, "unhandled message");
                }
            }
        }

        Ok(())
    }

    async fn handle_discover(&mut self, message: Message) -> Result<(), Error> {
        let requested_ip_address = get_opt!(message, RequestedIpAddress).copied();

        let target = SendTo::from_message(&message);

        if let Some(offer) = self.leases.get_offer(
            ClientIdentifier::from_message(&message),
            requested_ip_address,
        ) {
            let mut offer_reply = self.create_reply(&message, MessageType::Offer);
            offer_reply.set_yiaddr(offer.ip_address);
            self.send_message(&offer_reply, target).await?;

            let ack = self.create_reply(&message, MessageType::Ack);
            self.send_message(&ack, target).await?;
        }
        else {
            let nack = self.create_reply(&message, MessageType::Nak);
            self.send_message(&nack, target).await?;
        }

        Ok(())
    }

    async fn handle_request(&mut self, message: Message) -> Result<(), Error> {
        if let Some(server_identifier) = get_opt!(message, ServerIdentifier).copied() {
            if server_identifier != self.network_config.dhcp_server {
                // not for us.
                return Ok(());
            }
        }

        let requested_ip_address = get_opt!(message, RequestedIpAddress)
            .copied()
            .ok_or_else(|| InvalidRequest::NoRequestedIpAddress)?;

        let target = SendTo::from_message(&message);

        if let Some(lease) = self.leases.request_lease(
            ClientIdentifier::from_message(&message),
            message.chaddr().into(),
            requested_ip_address,
        ) {
            let lease_ip_address = lease.ip_address;
            let mut ack = self.create_reply(&message, MessageType::Ack);
            ack.set_yiaddr(lease_ip_address);
            let opts = ack.opts_mut();
            opts.insert(DhcpOption::AddressLeaseTime(LEASE_TIME));
            opts.insert(DhcpOption::SubnetMask(
                self.network_config.subnet.full_netmask(),
            ));
            opts.insert(DhcpOption::Router(vec![self.network_config.router]));
            opts.insert(DhcpOption::DomainNameServer(
                self.network_config.dns_servers.clone(),
            ));
            self.send_message(&ack, target).await?;
        }
        else {
            let nack = self.create_reply(&message, MessageType::Nak);
            self.send_message(&nack, target).await?;
        }

        Ok(())
    }

    fn create_reply(&self, request: &Message, message_type: MessageType) -> Message {
        let mut reply = Message::default();
        reply.set_opcode(Opcode::BootReply);
        reply.set_htype(HType::Eth);
        reply.set_xid(request.xid());
        reply.set_siaddr(self.network_config.router);
        reply.set_chaddr(request.chaddr());

        let opts = reply.opts_mut();
        opts.insert(DhcpOption::MessageType(message_type));
        opts.insert(DhcpOption::ServerIdentifier(
            self.network_config.dhcp_server,
        ));

        for request in get_opt!(request, ParameterRequestList)
            .map(|v| v.into_iter())
            .into_iter()
            .flatten()
        {
            match request {
                OptionCode::SubnetMask => {
                    opts.insert(DhcpOption::SubnetMask(
                        self.network_config.subnet.full_netmask(),
                    ));
                }
                OptionCode::Router => {
                    opts.insert(DhcpOption::Router(vec![self.network_config.router]));
                }
                OptionCode::DomainNameServer => {
                    opts.insert(DhcpOption::DomainNameServer(
                        self.network_config.dns_servers.clone(),
                    ));
                }
                OptionCode::AddressLeaseTime => {
                    opts.insert(DhcpOption::AddressLeaseTime(LEASE_TIME));
                }
                OptionCode::DomainName => {
                    opts.insert(DhcpOption::DomainName("skunk.local".to_owned()));
                }
                OptionCode::InterfaceMtu => {
                    opts.insert(DhcpOption::InterfaceMtu(1500));
                }
                OptionCode::BroadcastAddr => {
                    opts.insert(DhcpOption::BroadcastAddr(
                        self.network_config.subnet.broadcast_address(),
                    ));
                }
                OptionCode::CaptivePortal => {
                    // ignore these
                }
                _ => {
                    tracing::debug!("client requested unavailable parameter: {request:?}");
                }
            }
        }

        reply
    }

    async fn send_message(&mut self, message: &Message, target: SendTo) -> Result<(), Error> {
        let mut encoder = Encoder::new(&mut self.buf);
        message.encode(&mut encoder)?;

        tracing::debug!(message = ?MessageDebug(&message), ?target, "sending");

        self.socket
            .send_to(encoder.buffer(), target.to_socket_address())
            .await?;
        Ok(())
    }
}

impl Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("socket", &self.socket)
            .field("network_config", &self.network_config)
            .field("leases", &self.leases)
            .finish()
    }
}

#[derive(Clone, Copy, Debug)]
enum SendTo {
    Relay(Ipv4Addr),
    Client(Ipv4Addr),
    Broadcast,
}

impl SendTo {
    pub fn from_message(message: &Message) -> Self {
        if !message.giaddr().is_unspecified() {
            Self::Relay(message.giaddr())
        }
        else if !message.ciaddr().is_unspecified() {
            Self::Client(message.ciaddr())
        }
        else {
            // ideally we would check the broadcast bit here. but it's kind of difficult to
            // send an UDP packet to a mac address.
            Self::Broadcast
        }
    }

    pub fn to_socket_address(&self) -> (Ipv4Addr, u16) {
        match self {
            Self::Relay(ip_address) => (*ip_address, SERVER_PORT),
            Self::Client(ip_address) => (*ip_address, CLIENT_PORT),
            Self::Broadcast => (Ipv4Addr::BROADCAST, CLIENT_PORT),
        }
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

#[derive(Debug)]
struct Leases {
    leases: IndexMap<ClientIdentifier, Lease>,
    by_mac_address: HashMap<MacAddress, usize>,
    by_ip_address: HashMap<Ipv4Addr, usize>,
    available: BTreeSet<Ipv4Addr>,
}

impl Leases {
    pub fn new(pool: RangeInclusive<Ipv4Addr>) -> Self {
        let available: BTreeSet<Ipv4Addr> = pool.into_iter().collect();
        let n = available.len();
        Self {
            leases: IndexMap::with_capacity(n),
            by_mac_address: HashMap::with_capacity(n),
            by_ip_address: HashMap::with_capacity(n),
            available,
        }
    }

    pub fn get_offer(
        &self,
        client_identifier: ClientIdentifier,
        requested_ip_address: Option<Ipv4Addr>,
    ) -> Option<Offer> {
        if let Some(requested) = requested_ip_address {
            if let Some(index) = self.by_ip_address.get(&requested) {
                let (_, lease) = self.leases.get_index(*index).unwrap();
                if lease.client_identifier == client_identifier {
                    return Some(Offer {
                        ip_address: requested,
                    });
                }
            }
        }

        Some(Offer {
            ip_address: self.available.iter().next().copied()?,
        })
    }

    pub fn request_lease(
        &mut self,
        client_identifier: ClientIdentifier,
        mac_address: MacAddress,
        requested_ip_address: Ipv4Addr,
    ) -> Option<&Lease> {
        match self.leases.entry(client_identifier.clone()) {
            Entry::Occupied(entry) => {
                let lease = entry.into_mut();
                (lease.client_identifier == client_identifier).then_some(lease)
            }
            Entry::Vacant(entry) => {
                let index = entry.index();
                let lease = entry.insert(Lease {
                    mac_address,
                    ip_address: requested_ip_address,
                    client_identifier,
                });
                self.by_ip_address.insert(requested_ip_address, index);
                self.by_mac_address.insert(mac_address, index);
                Some(lease)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ClientIdentifier {
    Bytes(Bytes),
    MacAddress(MacAddress),
}

impl ClientIdentifier {
    pub fn from_bytes(bytes: impl Into<Bytes>) -> Self {
        Self::Bytes(bytes.into())
    }

    pub fn from_mac_address(mac_address: MacAddress) -> Self {
        Self::MacAddress(mac_address)
    }

    pub fn from_message(message: &Message) -> Self {
        get_opt!(message, ClientIdentifier)
            .map(|ident| Self::from_bytes(Bytes::copy_from_slice(ident)))
            .unwrap_or_else(|| Self::from_mac_address(message.chaddr().into()))
    }
}

#[derive(Debug)]
struct Lease {
    mac_address: MacAddress,
    ip_address: Ipv4Addr,
    client_identifier: ClientIdentifier,
}

#[derive(Debug)]
struct Offer {
    ip_address: Ipv4Addr,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct MacAddress(pub [u8; 6]);

impl Display for MacAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

impl Debug for MacAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MacAddress({self})")
    }
}

impl From<[u8; 6]> for MacAddress {
    fn from(value: [u8; 6]) -> Self {
        Self(value)
    }
}

impl From<&[u8]> for MacAddress {
    fn from(value: &[u8]) -> Self {
        Self(
            value
                .try_into()
                .expect("expected mac address to be exactly 6 bytes"),
        )
    }
}

struct MessageDebug<'a>(&'a Message);

impl<'a> Debug for MessageDebug<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Message")
            .field("type", get_opt!(&self.0, MessageType).unwrap())
            .field("xid", &self.0.xid())
            .field("ciaddr", &self.0.ciaddr())
            .field("yiaddr", &self.0.yiaddr())
            .field("siaddr", &self.0.siaddr())
            .field("giaddr", &self.0.giaddr())
            .field("chaddr", &MacAddress::from(self.0.chaddr()))
            .finish()
    }
}
