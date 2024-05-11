use std::{
    fmt::Display,
    num::NonZeroUsize,
    sync::atomic::{
        AtomicUsize,
        Ordering,
    },
};

use super::address::TcpAddress;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FilterId(NonZeroUsize);

impl FilterId {
    pub fn new() -> Self {
        static GEN: AtomicUsize = AtomicUsize::new(1);
        let id = GEN.fetch_add(1, Ordering::Relaxed);
        Self(NonZeroUsize::new(id).unwrap())
    }
}

#[derive(Debug)]
pub struct Filter {
    id: FilterId,
    hostname: String,
}

impl Filter {
    pub fn new(hostname: impl Display) -> Self {
        Filter {
            id: FilterId::new(),
            hostname: hostname.to_string(),
        }
    }

    pub fn matches(&self, address: &TcpAddress) -> bool {
        address.host.to_string() == self.hostname
    }
}

#[derive(Debug, Default)]
pub struct Filters {
    filters: Vec<Filter>,
}

impl Filters {
    pub fn push(&mut self, filter: Filter) {
        self.filters.push(filter);
    }

    pub fn matches(&self, address: &TcpAddress) -> Option<FilterId> {
        self.filters
            .iter()
            .find(|filter| filter.matches(address))
            .map(|filter| filter.id)
    }
}
