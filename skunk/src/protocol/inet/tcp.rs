use bitflags::bitflags;
use byst::{
    endianness::NetworkEndian,
    io::{
        BufReader,
        End,
        Read,
        Reader,
        ReaderExt,
    },
};

use crate::util::network_enum;

#[derive(Clone, Debug)]
pub struct Header {
    pub source_port: u16,
    pub destination_port: u16,
    pub sequence_number: u32,
    pub acknowledgment_number: u32,
    pub data_offset: u8,
    pub flags: Flags,
    pub window_size: u16,
    pub checksum: u16,
    pub urgent_pointer: u16,
    //pub options: O,
}

impl<R: Reader> Read<R, ()> for Header {
    type Error = InvalidHeader;

    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        let source_port = reader.read_with(NetworkEndian)?;
        let destination_port = reader.read_with(NetworkEndian)?;
        let sequence_number = reader.read_with(NetworkEndian)?;
        let acknowledgment_number = reader.read_with(NetworkEndian)?;
        let data_offset = reader.read::<u8>()? >> 4;
        let flags = Flags::from_bits_retain(reader.read()?);
        let window_size = reader.read_with(NetworkEndian)?;
        let checksum = reader.read_with(NetworkEndian)?;
        let urgent_pointer = reader.read_with(NetworkEndian)?;

        if data_offset < 5 || data_offset > 15 {
            return Err(InvalidHeader::InvalidDataOffset { data_offset });
        }

        // read options
        /*let options = {
            let reader = reader.limit((data_offset - 5) * 4);
            let options = vec![];
            while let Ok(kind) = reader.read::<OptionKind>().map_err(|End| ()) {
                let length = match kind {
                    OptionKind::End => {
                        options.push(Option::End);
                    }
                }
            }
        };*/

        Ok(Self {
            source_port,
            destination_port,
            sequence_number,
            acknowledgment_number,
            data_offset,
            flags,
            window_size,
            checksum,
            urgent_pointer,
            //options,
        })
    }
}

bitflags! {
    #[derive(Clone, Copy, Debug)]
    pub struct Flags: u8 {
        const CWR = 0b10000000;
        const ECE = 0b01000000;
        const URG = 0b00100000;
        const ACK = 0b00010000;
        const PSH = 0b00001000;
        const RST = 0b00000100;
        const SYN = 0b00000010;
        const FIN = 0b00000001;
    }
}

#[derive(Clone, Debug, Default)]
pub struct ReceivedOptions<B> {
    buf: B,
}

impl<R: BufReader> Read<R, ()> for ReceivedOptions<R::View> {
    type Error = InvalidOption;

    fn read(_reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        //Ok(Self {
        //    buf: reader.read()?,
        //})
        todo!();
    }
}

pub enum Option<D> {
    End,
    Nop,
    Other { kind: OptionKind, data: D },
}

/// See[1]
///
/// [1]: https://www.iana.org/assignments/tcp-parameters/tcp-parameters.xhtml#tcp-parameters-1
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Read)]
pub struct OptionKind(u8);

network_enum! {
    for OptionKind

    /// End of Options list
    END => 0x00;

    /// No-Operation
    NOP => 0x01;
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid TCP option")]
pub enum InvalidOption {}

#[derive(Debug, thiserror::Error)]
#[error("Invalid TCP header")]
pub enum InvalidHeader {
    #[error("TCP header incomplete")]
    Incomplete(#[from] End),

    #[error("Invalid data offset: {data_offset}")]
    InvalidDataOffset { data_offset: u8 },
}
