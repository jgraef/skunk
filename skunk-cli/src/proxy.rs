use crate::{
    config::Config,
    Error,
};

pub struct Proxy {}

impl Proxy {
    pub async fn from_config(_config: &Config) -> Result<(), Error> {
        todo!();
    }
}
