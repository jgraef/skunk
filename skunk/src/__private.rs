//! Re-exports for use by macros.

pub use std::{
    option::Option::{
        self,
        None,
        Some,
    },
    result::Result::{
        self,
        Err,
        Ok,
    },
};

pub mod rw {
    pub use crate::util::bytes::{
        endianness::{
            BigEndian,
            Endianness,
            LittleEndian,
            NativeEndian,
            NetworkEndian,
        },
        rw::{
            End,
            Full,
            Read,
            ReadXe,
            Write,
            WriteXe,
        },
    };
}
