#![allow(dead_code)]

mod client;
mod error;
mod flow;
mod socket;
mod util;

pub use self::{
    client::{
        Client,
        Connection,
    },
    error::Error,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Disconnected,
    Connected,
}
