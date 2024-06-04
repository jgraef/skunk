use super::error::{
    InvalidCommand,
    InvalidReply,
};

pub mod client;
pub mod server;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Connect,
    Bind,
}

impl TryFrom<u8> for Command {
    type Error = InvalidCommand;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(Self::Connect),
            0x02 => Ok(Self::Bind),
            _ => Err(InvalidCommand { value }),
        }
    }
}

impl From<Command> for u8 {
    fn from(value: Command) -> Self {
        match value {
            Command::Connect => 0x01,
            Command::Bind => 0x02,
        }
    }
}

pub enum Reply {
    Granted,
    Failed,
    IdentdConnectionRefused,
    IdentdMismatch,
}

impl TryFrom<u8> for Reply {
    type Error = InvalidReply;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            90 => Ok(Self::Granted),
            91 => Ok(Self::Failed),
            92 => Ok(Self::IdentdConnectionRefused),
            93 => Ok(Self::IdentdMismatch),
            _ => Err(InvalidReply { value }),
        }
    }
}

impl From<Reply> for u8 {
    fn from(value: Reply) -> Self {
        match value {
            Reply::Granted => 90,
            Reply::Failed => 91,
            Reply::IdentdConnectionRefused => 92,
            Reply::IdentdMismatch => 93,
        }
    }
}
