use byst::io::{
    End,
    Limit,
    Read,
    Reader,
    ReaderExt,
};

#[derive(Clone, Copy, Debug, Read)]
pub struct Header {
    #[byst(network)]
    pub source_port: u16,

    #[byst(network)]
    pub destination_port: u16,

    #[byst(network)]
    pub length: u16,

    #[byst(network)]
    pub checksum: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct Packet<P> {
    pub header: Header,
    pub payload: P,
}

impl<R: Reader, P, E> Read<R, ()> for Packet<P>
where
    P: for<'r> Read<Limit<&'r mut R>, (), Error = E>,
{
    type Error = InvalidPacket<E>;

    fn read(reader: &mut R, _params: ()) -> Result<Self, Self::Error> {
        let header: Header = reader.read()?;

        let payload = reader
            .limit(header.length.into())
            .read()
            .map_err(InvalidPacket::Payload)?;

        Ok(Self { header, payload })
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid UDP packet")]
pub enum InvalidPacket<P> {
    Incomplete(#[from] End),
    Payload(#[source] P),
}
