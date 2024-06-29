#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use self::unix::{
    interface_from_name,
    list_interfaces,
    Socket,
};
