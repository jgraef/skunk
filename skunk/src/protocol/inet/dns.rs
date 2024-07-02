//! Domain Name Service
//!
//! - [RFC 1034](https://datatracker.ietf.org/doc/html/rfc1034)
//! - [RFC 1035](https://datatracker.ietf.org/doc/html/rfc1035) (message format)

use std::{
    convert::Infallible,
    net::Ipv4Addr,
};

use byst::{
    endianness::NetworkEndian,
    io::{
        BufReader,
        End,
        Limit,
        Read,
        Reader,
        ReaderExt,
    },
    Buf,
    Bytes,
};
use smallvec::SmallVec;

use crate::util::network_enum;

/// DNS message.
///
/// See [RFC 1034 Section 4][1]
///
/// [1]: https://datatracker.ietf.org/doc/html/rfc1035#section-4
#[derive(Clone, Debug)]
pub struct Message {
    pub header: Header,
    pub questions: SmallVec<[Question; 1]>,
    pub answers: Vec<ResourceRecord>,
    pub authority: Vec<ResourceRecord>,
    pub additional: Vec<ResourceRecord>,
}

impl<R: BufReader> Read<R, ()> for Message
where
    Bytes: Read<R, (), Error = Infallible>,
    Bytes: for<'r> Read<Limit<&'r mut R>, (), Error = Infallible>,
{
    type Error = InvalidMessage;

    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        let rest = reader.peek_rest();
        let pointer_base = PointerBase(&rest);

        let header = reader.read::<Header>()?;

        let mut questions = SmallVec::with_capacity(header.num_questions.into());
        for _ in 0..header.num_questions {
            questions.push(reader.read_with(pointer_base)?);
        }

        let mut answers = Vec::with_capacity(header.num_answers.into());
        for _ in 0..header.num_answers {
            answers.push(reader.read_with(pointer_base)?);
        }

        let mut authority = Vec::with_capacity(header.num_authority.into());
        for _ in 0..header.num_authority {
            authority.push(reader.read_with(pointer_base)?);
        }

        let mut additional = Vec::with_capacity(header.num_additional.into());
        for _ in 0..header.num_additional {
            additional.push(reader.read_with(pointer_base)?);
        }

        Ok(Self {
            header,
            questions,
            answers,
            authority,
            additional,
        })
    }
}

#[derive(Clone, Copy)]
struct PointerBase<B: Buf + Copy>(B);

#[derive(Clone, Copy, Debug, Read)]
pub struct Header {
    #[byst(network)]
    pub transaction_id: u16,
    pub flags: Flags,
    #[byst(network)]
    pub num_questions: u16,
    #[byst(network)]
    pub num_answers: u16,
    #[byst(network)]
    pub num_authority: u16,
    #[byst(network)]
    pub num_additional: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct Flags {
    pub qr: Qr,
    pub opcode: Opcode,
    pub aa: bool,
    pub tc: bool,
    pub rd: bool,
    pub ra: bool,
    pub z: u8,
    pub rcode: ResponseCode,
}

impl<R: Reader> Read<R, ()> for Flags {
    type Error = R::Error;

    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        let value = reader.read_with::<u16, _>(NetworkEndian)?;
        let qr = if value & 0x0001 == 0 {
            Qr::Query
        }
        else {
            Qr::Reply
        };
        let opcode = Opcode((value & 0x001e) as u8);
        let aa = value & 0x0020 != 0;
        let tc = value & 0x0040 != 0;
        let rd = value & 0x0080 != 0;
        let ra = value & 0x0100 != 0;
        let z = (value & 0x0e00 >> 9) as u8;
        let rcode = ResponseCode((value >> 12) as u8);
        Ok(Self {
            qr,
            opcode,
            aa,
            tc,
            rd,
            ra,
            z,
            rcode,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Qr {
    Query,
    Reply,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Opcode(u8);

network_enum! {
    for Opcode;

    QUERY => 0;
    IQUERY => 1;
    STATUS => 2;
}

impl From<Opcode> for u8 {
    #[inline]
    fn from(value: Opcode) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResponseCode(u8);

network_enum! {
    for ResponseCode: Debug;

    /// No error condition
    NO_ERROR => 0;

    /// Format error - The name server was unable to interpret the query.
    FORMAT_ERROR => 1;

    /// Server failure - The name server was unable to process this query due to a problem with the name server.
    SERVER_FAILURE => 2;

    /// Name Error - Meaningful only for responses from an authoritative name server, this code signifies that the domain name referenced in the query does not exist.
    NAME_ERROR => 3;

    /// Not Implemented - The name server does not support the requested kind of query.
    NOT_IMPLEMENTED => 4;

    /// Refused - The name server refuses to perform the specified operation for policy reasons.  For example, a name server may not wish to provide the information to the particular requester, or a name server may not wish to perform a particular operation (e.g., zone transfer) for particular data.
    REFUSED => 5;
}

impl From<ResponseCode> for u8 {
    #[inline]
    fn from(value: ResponseCode) -> Self {
        value.0
    }
}

#[derive(Clone, Debug)]
pub struct Question {
    pub qname: Name,
    pub qtype: QuestionType,
    pub qclass: QuestionClass,
}

impl<R: BufReader, B: Buf + Copy> Read<R, PointerBase<B>> for Question {
    type Error = InvalidMessage;

    fn read(reader: &mut R, base: PointerBase<B>) -> Result<Self, Self::Error> {
        Ok(Self {
            qname: reader.read_with(base)?,
            qtype: reader.read()?,
            qclass: reader.read()?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ResourceRecord {
    pub name: Name,
    pub r#type: RecordType,
    pub class: RecordClass,
    pub ttl: i32,
    pub rdata: RecordData,
}

impl<R: BufReader, B: Buf + Copy> Read<R, PointerBase<B>> for ResourceRecord
where
    Bytes: for<'r> Read<Limit<&'r mut R>, (), Error = Infallible>,
{
    type Error = InvalidMessage;

    fn read(reader: &mut R, base: PointerBase<B>) -> Result<Self, Self::Error> {
        let name = reader.read_with(base)?;
        let r#type = reader.read()?;
        let class = reader.read()?;
        let ttl = reader.read_with(NetworkEndian)?;
        let rdlength: u16 = reader.read_with(NetworkEndian)?;
        let rdata = reader
            .limit(rdlength.into())
            .read_with((base, r#type, class))?;

        Ok(Self {
            name,
            r#type,
            class,
            ttl,
            rdata,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Name {
    inner: String,
}

impl<R: BufReader, B: Buf + Copy> Read<R, PointerBase<B>> for Name {
    type Error = InvalidMessage;

    fn read(reader: &mut R, base: PointerBase<B>) -> Result<Self, Self::Error> {
        struct NameReader {
            name: Vec<u8>,
            recursion_limit: usize,
            total_length: usize,
            is_first: bool,
            last_was_all_numeric: bool,
        }

        impl NameReader {
            pub fn read(
                &mut self,
                mut reader: impl BufReader,
                base: &impl Buf,
            ) -> Result<(), InvalidMessage> {
                loop {
                    let value = reader.read::<u8>()?;
                    if value == 0 {
                        // end
                        return Ok(());
                    }

                    let flags = value >> 6;
                    if flags == 3 {
                        // pointer
                        self.recursion_limit -= 1;
                        if self.recursion_limit == 0 {
                            return Err(InvalidName::PointerLimit.into());
                        }

                        let value2 = reader.read::<u8>()?;
                        let pointer = ((value & 0x3f) as u16) << 8 | value2 as u16;

                        let mut reader = base.reader();
                        reader.advance(pointer.into())?;
                        return self.read(reader, base);
                    }
                    else if flags == 0 {
                        // label
                        let length = usize::from(value & 0x3f);
                        self.total_length += usize::from(length);
                        if self.total_length > 255 {
                            return Err(InvalidName::TooLong {
                                length: self.total_length,
                                limit: 255,
                            }
                            .into());
                        }

                        if self.is_first {
                            self.is_first = false;
                        }
                        else {
                            self.name.push(b'.');
                        }

                        //let view = reader.view(length)?;
                        let view = BufReader::view(&mut reader, length)?;
                        is_valid_label(&view, &mut self.last_was_all_numeric)?;

                        let mut view = view.reader();
                        while let Some(chunk) = view.peek_chunk() {
                            self.name.extend(chunk);
                            view.advance(chunk.len()).unwrap();
                        }
                    }
                    else {
                        // reserved for future use, invalid
                        return Err(InvalidName::InvalidFlags { flags }.into());
                    }
                }
            }
        }

        let mut name_reader = NameReader {
            name: Vec::with_capacity(256),
            recursion_limit: 256,
            total_length: 0,
            is_first: true,
            last_was_all_numeric: false,
        };

        name_reader.read(reader, &base.0)?;

        if name_reader.last_was_all_numeric {
            return Err(InvalidName::NumericTld.into());
        }

        Ok(Self {
            inner: String::from_utf8(name_reader.name).unwrap(),
        })
    }
}

/// [`TYPE` values][1]
///
/// [1]: https://datatracker.ietf.org/doc/html/rfc1035#section-3.2.2
#[derive(Clone, Copy, PartialEq, Eq, Hash, Read)]
pub struct RecordType(#[byst(network)] pub u16);

network_enum! {
    for RecordType: Debug;

    /// a host address
    A => 1;

    /// an authoritative name server
    NS => 2;

    /// a mail destination (Obsolete - use MX)
    MD => 3;

    /// a mail forwarder (Obsolete - use MX)
    MF => 4;

    /// the canonical name for an alias
    CNAME => 5;

    /// marks the start of a zone of authority
    SOA => 6;

    /// a mailbox domain name (EXPERIMENTAL)
    MB => 7;

    /// a mail group member (EXPERIMENTAL)
    MG => 8;

    /// a mail rename domain name (EXPERIMENTAL)
    MR => 9;

    /// a null RR (EXPERIMENTAL)
    NULL => 10;

    /// a well known service description
    WKS => 11;

    /// a domain name pointer
    PTR => 12;

    /// host information
    HINFO => 13;

    /// mailbox or mail list information
    MINFO => 14;

    /// mail exchange
    MX => 15;

    /// text strings
    TXT => 16;
}

/// [`QTYPE` values][1]
///
/// [1]: https://datatracker.ietf.org/doc/html/rfc1035#section-3.2.3
#[derive(Clone, Copy, PartialEq, Eq, Hash, Read)]
pub struct QuestionType(#[byst(network)] pub u16);

network_enum! {
    for QuestionType: Debug;

    /// a host address
    A => 1;

    /// an authoritative name server
    NS => 2;

    /// a mail destination (Obsolete - use MX)
    MD => 3;

    /// a mail forwarder (Obsolete - use MX)
    MF => 4;

    /// the canonical name for an alias
    CNAME => 5;

    /// marks the start of a zone of authority
    SOA => 6;

    /// a mailbox domain name (EXPERIMENTAL)
    MB => 7;

    /// a mail group member (EXPERIMENTAL)
    MG => 8;

    /// a mail rename domain name (EXPERIMENTAL)
    MR => 9;

    /// a null RR (EXPERIMENTAL)
    NULL => 10;

    /// a well known service description
    WKS => 11;

    /// a domain name pointer
    PTR => 12;

    /// host information
    HINFO => 13;

    /// mailbox or mail list information
    MINFO => 14;

    /// mail exchange
    MX => 15;

    /// text strings
    TXT => 16;

    /// A request for a transfer of an entire zone
    AXFR => 252;

    /// A request for mailbox-related records (MB, MG or MR)
    MAILB => 253;

    /// A request for mail agent RRs (Obsolete - see MX)
    MAILA => 254;

    /// A request for all records
    ANY => 255;
}

impl From<RecordType> for QuestionType {
    #[inline]
    fn from(value: RecordType) -> Self {
        Self(value.0)
    }
}

/// [`CLASS` values][1]
///
/// [1]: https://datatracker.ietf.org/doc/html/rfc1035#section-3.2.4
#[derive(Clone, Copy, PartialEq, Eq, Hash, Read)]
pub struct RecordClass(#[byst(network)] pub u16);

network_enum! {
    for RecordClass: Debug;

    /// the Internet
    IN => 1;

    /// the CSNET class (Obsolete - used only for examples in some obsolete RFCs)
    CS => 2;

    /// the CHAOS class
    CH => 3;

    /// Hesiod [Dyer 87]
    HS => 4;
}

/// [`QCLASS` values][1]
///
/// [1]: https://datatracker.ietf.org/doc/html/rfc1035#section-3.2.5
#[derive(Clone, Copy, PartialEq, Eq, Hash, Read)]
pub struct QuestionClass(#[byst(network)] pub u16);

network_enum! {
    for QuestionClass: Debug;

    /// the Internet
    IN => 1;

    /// the CSNET class (Obsolete - used only for examples in some obsolete RFCs)
    CS => 2;

    /// the CHAOS class
    CH => 3;

    /// Hesiod [Dyer 87]
    HS => 4;

    /// any class
    ANY => 255;
}

#[derive(Clone, Debug)]
pub enum RecordData {
    Cname {
        cname: Name,
    },
    Mx {
        preference: u16,
        exchange: Name,
    },
    Null {
        data: Bytes,
    },
    Ns {
        ns_dname: Name,
    },
    Ptr {
        ptr_dname: Name,
    },
    Soa {
        mname: Name,
        rname: Name,
        serial: u32,
        refresh: u32,
        retry: u32,
        expire: u32,
        minimum: u32,
    },
    Txt {
        txt_data: Bytes,
    },
    A {
        address: Ipv4Addr,
    },
    Wks {
        address: Ipv4Addr,
        protocol: u8,
        bitmap: Bytes,
    },
    Unknown {
        r#type: RecordType,
        class: RecordClass,
    },
}

impl<R: BufReader, B: Buf + Copy> Read<R, (PointerBase<B>, RecordType, RecordClass)> for RecordData
where
    Bytes: Read<R, (), Error = Infallible>,
{
    type Error = InvalidMessage;

    fn read(
        reader: &mut R,
        (base, r#type, class): (PointerBase<B>, RecordType, RecordClass),
    ) -> Result<Self, Self::Error> {
        match (r#type, class) {
            (RecordType::CNAME, _) => {
                Ok(Self::Cname {
                    cname: reader.read_with(base)?,
                })
            }
            (RecordType::MX, _) => {
                Ok(Self::Mx {
                    preference: reader.read_with(NetworkEndian)?,
                    exchange: reader.read_with(base)?,
                })
            }
            (RecordType::NULL, _) => {
                Ok(Self::Null {
                    data: reader.read()?,
                })
            }
            (RecordType::NS, _) => {
                Ok(Self::Ns {
                    ns_dname: reader.read_with(base)?,
                })
            }
            (RecordType::PTR, _) => {
                Ok(Self::Ptr {
                    ptr_dname: reader.read_with(base)?,
                })
            }
            (RecordType::SOA, _) => {
                Ok(Self::Soa {
                    mname: reader.read_with(base)?,
                    rname: reader.read_with(base)?,
                    serial: reader.read_with(NetworkEndian)?,
                    refresh: reader.read_with(NetworkEndian)?,
                    retry: reader.read_with(NetworkEndian)?,
                    expire: reader.read_with(NetworkEndian)?,
                    minimum: reader.read_with(NetworkEndian)?,
                })
            }
            (RecordType::TXT, _) => {
                Ok(Self::Txt {
                    txt_data: reader.read()?,
                })
            }
            (RecordType::A, RecordClass::IN) => {
                Ok(Self::A {
                    address: reader.read()?,
                })
            }
            (RecordType::WKS, RecordClass::IN) => {
                Ok(Self::Wks {
                    address: reader.read()?,
                    protocol: reader.read()?,
                    bitmap: reader.read()?,
                })
            }
            _ => Ok(Self::Unknown { r#type, class }),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid DNS message")]
pub enum InvalidMessage {
    Incomplete(#[from] End),
    InvalidName(#[from] InvalidName),
}

impl From<Infallible> for InvalidMessage {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidName {
    #[error("Name too long: {length} > {limit}")]
    TooLong { length: usize, limit: usize },
    #[error("Pointer limit reached")]
    PointerLimit,
    #[error("Invalid flags: {flags:#b}")]
    InvalidFlags { flags: u8 },
    #[error("Invalid character in label: {:?}", char::from(*character))]
    InvalidCharacter { character: u8 },
    #[error("TLD can't be all numeric")]
    NumericTld,
}

fn is_valid_label(label: impl Buf, is_all_numeric: &mut bool) -> Result<(), InvalidName> {
    let mut reader = label.reader();

    let Some(mut chunk) = reader.peek_chunk()
    else {
        return Ok(());
    };
    if chunk[0] == b'-' {
        return Err(InvalidName::InvalidCharacter {
            character: chunk[0],
        });
    }

    *is_all_numeric = true;

    loop {
        match chunk[0] {
            b'a'..b'z' | b'A'..b'Z' | b'-' => *is_all_numeric = false,
            b'0'..b'9' => {}
            _ => {
                return Err(InvalidName::InvalidCharacter {
                    character: chunk[0],
                })
            }
        }

        reader.advance(1).unwrap();
        let Some(c) = reader.peek_chunk()
        else {
            break;
        };
        chunk = c;
    }

    Ok(())
}
