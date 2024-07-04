pub mod arp;
pub mod dhcp;
pub mod dns;
pub mod ethernet;
pub mod icmp;
pub mod ipv4;
pub mod ipv6;
mod mac_address;
pub mod sll;
pub mod tcp;
pub mod udp;

pub use std::net::{
    IpAddr,
    Ipv4Addr,
    Ipv6Addr,
};

pub use ip_network::{
    IpNetwork,
    Ipv4Network,
    Ipv6Network,
};
pub use skunk_macros::{
    ip_address,
    ipv4_address,
    ipv6_address,
    mac_address,
};

pub use self::mac_address::MacAddress;

#[cfg(test)]
mod tests {
    use std::net::{
        IpAddr,
        Ipv4Addr,
        Ipv6Addr,
    };

    use ip_network::{
        IpNetwork,
        Ipv4Network,
        Ipv6Network,
    };
    use skunk_macros::{
        ip_address,
        ip_network,
        ipv4_address,
        ipv4_network,
        ipv6_address,
        ipv6_network,
        mac_address,
    };

    use crate::protocol::inet::MacAddress;

    #[test]
    fn ipv4_address_macro_produces_valid_values() {
        assert_eq!(ipv4_address!("127.0.0.1"), Ipv4Addr::new(127, 0, 0, 1));
    }

    #[test]
    fn ipv6_address_macro_produces_valid_values() {
        assert_eq!(
            ipv6_address!("2001:db8::8a2e:370:7334"),
            "2001:db8::8a2e:370:7334".parse::<Ipv6Addr>().unwrap()
        );
    }

    #[test]
    fn ip_address_macro_produces_valid_values() {
        assert_eq!(
            ip_address!("127.0.0.1"),
            "127.0.0.1".parse::<IpAddr>().unwrap()
        );
        assert_eq!(
            ip_address!("2001:db8::8a2e:370:7334"),
            "2001:db8::8a2e:370:7334".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn ipv4_network_macro_produces_valid_values() {
        assert_eq!(
            ipv4_network!("192.168.1.0/24"),
            "192.168.1.0/24".parse::<Ipv4Network>().unwrap()
        );
    }

    #[test]
    fn ipv6_network_macro_produces_valid_values() {
        assert_eq!(
            ipv6_network!("2001:db8::/32"),
            "2001:db8::/32".parse::<Ipv6Network>().unwrap()
        );
    }

    #[test]
    fn ip_network_macro_produces_valid_values() {
        assert_eq!(
            ip_network!("192.168.1.0/24"),
            "192.168.1.0/24".parse::<IpNetwork>().unwrap()
        );
        assert_eq!(
            ip_network!("2001:db8::/32"),
            "2001:db8::/32".parse::<IpNetwork>().unwrap()
        );
    }

    #[test]
    fn mac_address_macro_produces_valid_values() {
        let ip = mac_address!("04:d9:f5:58:f2:f7");
        assert_eq!(ip, MacAddress([0x04, 0xd9, 0xf5, 0x58, 0xf2, 0xf7]));
    }
}
