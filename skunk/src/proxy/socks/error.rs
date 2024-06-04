#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("invalid version: {0}")]
    InvalidVersion(u8),

    #[error("invalid command")]
    InvalidCommand(#[from] InvalidCommand),

    #[error("invalid hostname")]
    InvalidHostName,

    #[error("authentication failed")]
    AuthenticationFailed,

    #[error("invalid auth method")]
    InvalidAuthMethod(#[from] super::v5::InvalidAuthMethod),

    #[error("invalid request")]
    InvalidRequest,

    #[error("invalid address type")]
    InvalidAddressType(#[from] super::v5::InvalidAddressType),
}

#[derive(Debug, thiserror::Error)]
#[error("invalid command: 0x{value:02x}")]
pub struct InvalidCommand {
    pub value: u8,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid reply: 0x{value:02x}")]
pub struct InvalidReply {
    pub value: u8,
}
