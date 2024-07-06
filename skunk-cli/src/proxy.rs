use crate::{
    config::Config,
    Error,
};

pub enum Capture {
    SocksProxy,
    HttpProxy,
    Interface,
}

pub struct Proxy {}

impl Proxy {
    pub async fn from_config(_config: &Config) -> Result<(), Error> {
        todo!();
    }
}
