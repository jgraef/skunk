//! Re-exports for use by macros.

pub use std::{
    convert::From,
    option::Option::{
        self,
        None,
        Some,
    },
    primitive::usize,
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
