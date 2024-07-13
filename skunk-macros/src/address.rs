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
use proc_macro2::TokenStream;
use proc_macro_error::abort;
use quote::quote;
use syn::LitStr;

pub fn ipv4_address(input: LitStr) -> TokenStream {
    let ip_addr: Ipv4Addr = input
        .value()
        .parse()
        .unwrap_or_else(|e| abort!(input.span(), "{}", e));
    let octets = ip_addr.octets();
    quote! {
        ::std::net::Ipv4Addr::new(#(#octets),*)
    }
}

pub fn ipv6_address(input: LitStr) -> TokenStream {
    let ip_addr: Ipv6Addr = input
        .value()
        .parse()
        .unwrap_or_else(|e| abort!(input.span(), "{}", e));
    let segments = ip_addr.segments();
    quote! {
        ::std::net::Ipv6Addr::new(#(#segments),*)
    }
}

pub fn ip_address(input: LitStr) -> TokenStream {
    let ip_addr: IpAddr = input
        .value()
        .parse()
        .unwrap_or_else(|e| abort!(input.span(), "{}", e));
    match ip_addr {
        IpAddr::V4(ipv4_addr) => {
            let octets = ipv4_addr.octets();
            quote! {
                ::std::net::IpAddr::V4(::std::net::Ipv4Addr::new(#(#octets),*))
            }
        }
        IpAddr::V6(ipv6_addr) => {
            let segments = ipv6_addr.segments();
            quote! {
                ::std::net::IpAddr::V6(::std::net::Ipv6Addr::new(#(#segments),*))
            }
        }
    }
}

pub fn ipv4_network(input: LitStr) -> TokenStream {
    let ip_network: Ipv4Network = input
        .value()
        .parse()
        .unwrap_or_else(|e| abort!(input.span(), "{}", e));
    let octets = ip_network.network_address().octets();
    let netmask = ip_network.netmask();
    quote! {
        ::ip_network::Ipv4Network::new(::std::net::Ipv4Addr::new(#(#octets),*), #netmask).unwrap()
    }
}

pub fn ipv6_network(input: LitStr) -> TokenStream {
    let ip_network: Ipv6Network = input
        .value()
        .parse()
        .unwrap_or_else(|e| abort!(input.span(), "{}", e));
    let segments = ip_network.network_address().segments();
    let netmask = ip_network.netmask();
    quote! {
        ::ip_network::Ipv6Network::new(::std::net::Ipv6Addr::new(#(#segments),*), #netmask).unwrap()
    }
}

pub fn ip_network(input: LitStr) -> TokenStream {
    let ip_network: IpNetwork = input
        .value()
        .parse()
        .unwrap_or_else(|e| abort!(input.span(), "{}", e));
    match ip_network {
        IpNetwork::V4(ip_network) => {
            let octets = ip_network.network_address().octets();
            let netmask = ip_network.netmask();
            quote! {
                ::ip_network::IpNetwork::V4(::ip_network::Ipv4Network::new(::std::net::Ipv4Addr::new(#(#octets),*), #netmask).unwrap())
            }
        }
        IpNetwork::V6(ip_network) => {
            let segments = ip_network.network_address().segments();
            let netmask = ip_network.netmask();
            quote! {
                ::ip_network::IpNetwork::V6(::ip_network::Ipv6Network::new(::std::net::Ipv6Addr::new(#(#segments),*), #netmask).unwrap())
            }
        }
    }
}

pub fn mac_address(input: LitStr) -> TokenStream {
    let s = input.value();
    let mac_addr =
        parse_mac_address(&s).unwrap_or_else(|| abort!(input.span(), "Invalid MAC address: {}", s));
    quote! {
        ::skunk::protocol::inet::MacAddress([#(#mac_addr),*])
    }
}

#[inline]
fn parse_mac_address(mut s: &str) -> Option<[u8; 6]> {
    #[inline]
    fn hex(s: &mut &str) -> Option<u8> {
        let (a, b) = s.split_at(2);
        let a = u8::from_str_radix(a, 16).ok()?;
        *s = b;
        Some(a)
    }

    #[inline]
    fn colon(s: &mut &str) -> Option<()> {
        let (a, b) = s.split_at(1);
        *s = b;
        (a == ":").then_some(())
    }

    if !s.is_ascii() || s.len() != 17 {
        return None;
    }

    let mut buf = [0u8; 6];

    buf[0] = hex(&mut s)?;
    for b in &mut buf[1..6] {
        colon(&mut s)?;
        *b = hex(&mut s)?;
    }

    Some(buf)
}
