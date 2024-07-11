#[cfg(target_os = "linux")]
mod unix;

#[cfg(target_os = "linux")]
pub use self::unix::{
    interface_from_name,
    list_interfaces,
    Socket,
};

#[cfg(not(target_os = "linux"))]
mod stub;

#[cfg(not(target_os = "linux"))]
pub use self::stub::{
    interface_from_name,
    list_interfaces,
    Socket,
};
