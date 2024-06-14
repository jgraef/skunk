use byst::{
    endianness::NetworkEndian,
    rw::{
        End,
        Full,
        Read,
        ReadIntoBuf,
        ReadXe,
        Write,
        WriteFromBuf,
        WriteXe,
    },
};

/// Vlan tagged ethernet frames[1]
///
/// [1]: https://en.wikipedia.org/wiki/IEEE_802.1Q
#[derive(Clone, Copy, Debug)]
pub struct VlanTag {
    pub priority_code_point: PriorityCodePoint,
    pub drop_eligible: bool,
    pub vlan_identifier: VlanIdentifier,
}

impl<R: ReadIntoBuf> Read<R> for VlanTag {
    fn read(reader: R) -> Result<Self, End> {
        let value: u16 = ReadXe::<_, NetworkEndian>::read(reader)?;
        let priority_code_point = PriorityCodePoint((value >> 13) as u8);
        let drop_eligible = value & 0x1000 != 0;
        let vlan_identifier = VlanIdentifier(value & 0xfff);
        Ok(Self {
            priority_code_point,
            drop_eligible,
            vlan_identifier,
        })
    }
}

impl<W: WriteFromBuf> Write<W> for VlanTag {
    fn write(&self, writer: W) -> Result<(), Full> {
        let value = (self.priority_code_point.0 as u16) << 13
            & self.drop_eligible.then_some(0x1000).unwrap_or_default()
            & self.vlan_identifier.0;
        WriteXe::<_, NetworkEndian>::write(&value, writer)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PriorityCodePoint(u8);

impl From<PriorityCodePoint> for u8 {
    #[inline]
    fn from(value: PriorityCodePoint) -> Self {
        value.0
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid VLAN priority code point: 0x{value:02x}")]
pub struct InvalidPriorityCodePoint {
    pub value: u8,
}

impl TryFrom<u8> for PriorityCodePoint {
    type Error = InvalidPriorityCodePoint;

    #[inline]
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        (value & 0xf8 == 0)
            .then_some(Self(value))
            .ok_or(InvalidPriorityCodePoint { value })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VlanIdentifier(u16);

impl From<VlanIdentifier> for u16 {
    #[inline]
    fn from(value: VlanIdentifier) -> Self {
        value.0
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid VLAN identifier: 0x{value:04x}")]
pub struct InvalidVlanIdentifier {
    pub value: u16,
}

impl TryFrom<u16> for VlanIdentifier {
    type Error = InvalidVlanIdentifier;

    #[inline]
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        (value & 0xf000 == 0)
            .then_some(Self(value))
            .ok_or(InvalidVlanIdentifier { value })
    }
}
