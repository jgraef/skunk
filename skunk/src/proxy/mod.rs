use std::sync::Arc;

use crate::address::TcpAddress;

//pub mod http;
#[cfg(feature = "socks")]
pub mod socks;

pub trait ProxySource {
    fn target_address(&self) -> &TcpAddress;
}

impl<S: ProxySource> ProxySource for &S {
    fn target_address(&self) -> &TcpAddress {
        (*self).target_address()
    }
}

impl<S: ProxySource> ProxySource for &mut S {
    fn target_address(&self) -> &TcpAddress {
        (self as &S).target_address()
    }
}

impl<S: ProxySource> ProxySource for Arc<S> {
    fn target_address(&self) -> &TcpAddress {
        self.as_ref().target_address()
    }
}
