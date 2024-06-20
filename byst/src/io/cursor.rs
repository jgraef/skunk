use super::{
    read::{
        End,
        Read,
        ReadIntoBuf,
    },
    write::{
        Full,
        WriteFromBuf,
    },
    Position,
    Remaining,
    Skip,
};
use crate::{
    buf::{
        copy::copy,
        Buf,
        BufMut,
        Length as _,
    },
    range::Range,
    Bytes,
};

/// A reader and writer that reads and writes from and to a [`Buf`].
#[derive(Clone, Debug)]
pub struct Cursor<B> {
    buf: B,
    offset: usize,
}

impl<B> Cursor<B> {
    #[inline]
    pub fn new(buf: B) -> Self {
        Self::with_offset(buf, 0)
    }

    #[inline]
    pub fn with_offset(buf: B, offset: usize) -> Self {
        Self { buf, offset }
    }

    #[inline]
    pub fn into_inner(self) -> B {
        self.buf
    }
}

impl<B: Buf> Cursor<B> {
    #[inline]
    fn get_range(&self, n: usize) -> Range {
        Range::default().with_start(self.offset).with_length(n)
    }
}

impl<B: Buf> ReadIntoBuf for Cursor<B> {
    type Error = End;

    fn read_into_buf<D: BufMut>(&mut self, buf: D) -> Result<(), End> {
        let n = buf.len();
        let range = self.get_range(n);
        copy(buf, .., &self.buf, range).map_err(End::from_copy_error)?;
        self.offset += n;
        Ok(())
    }
}

impl<B: BufMut> WriteFromBuf for Cursor<B> {
    fn write_from_buf<S: Buf>(&mut self, source: S) -> Result<(), Full> {
        let n = source.len();
        let range = self.get_range(n);
        let total_copied = copy(&mut self.buf, range, source, ..).map_err(Full::from_copy_error)?;
        assert_eq!(total_copied, n);
        self.offset += n;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, derive_more::From, derive_more::Into)]
pub struct Length(pub usize);

impl<'b> Read<Cursor<&'b [u8]>, Length> for &'b [u8] {
    type Error = End;

    fn read(reader: &mut Cursor<&'b [u8]>, parameters: Length) -> Result<Self, End> {
        let range = Range::default()
            .with_start(reader.offset)
            .with_length(parameters.0);
        let view = reader
            .buf
            .view(range)
            .map_err(End::from_range_out_of_bounds)?;
        reader.offset += parameters.0;
        Ok(view)
    }
}

impl<'b> Read<Cursor<&'b [u8]>, ()> for &'b [u8] {
    type Error = End;

    fn read(reader: &mut Cursor<&'b [u8]>, _parameters: ()) -> Result<Self, End> {
        let range = Range::default().with_start(reader.offset);
        let view = reader
            .buf
            .view(range)
            .map_err(End::from_range_out_of_bounds)?;
        reader.offset += view.len();
        Ok(view)
    }
}

impl Read<Cursor<Bytes>, Length> for Bytes {
    type Error = End;

    fn read(reader: &mut Cursor<Bytes>, parameters: Length) -> Result<Self, End> {
        let range = Range::default()
            .with_start(reader.offset)
            .with_length(parameters.0);
        let view = reader
            .buf
            .view(range)
            .map_err(End::from_range_out_of_bounds)?;
        reader.offset += parameters.0;
        Ok(view)
    }
}

impl Read<Cursor<Bytes>, ()> for Bytes {
    type Error = End;

    fn read(reader: &mut Cursor<Bytes>, _parameters: ()) -> Result<Self, End> {
        let range = Range::default().with_start(reader.offset);
        let view = reader
            .buf
            .view(range)
            .map_err(End::from_range_out_of_bounds)?;
        reader.offset += view.len();
        Ok(view)
    }
}

impl<B: Buf> Skip for Cursor<B> {
    fn skip(&mut self, n: usize) -> Result<(), End> {
        let range = self.get_range(n);
        if self.buf.contains(range) {
            self.offset += n;
            Ok(())
        }
        else {
            Err(End)
        }
    }
}

impl<B> AsRef<B> for Cursor<B> {
    #[inline]
    fn as_ref(&self) -> &B {
        &self.buf
    }
}

impl<B> AsMut<B> for Cursor<B> {
    #[inline]
    fn as_mut(&mut self) -> &mut B {
        &mut self.buf
    }
}

impl<B: Buf> Remaining for Cursor<B> {
    #[inline]
    fn remaining(&self) -> usize {
        self.buf.len() - self.offset
    }
}

impl<B: Buf> Position for Cursor<B> {
    #[inline]
    fn position(&self) -> usize {
        self.offset
    }

    #[inline]
    fn set_position(&mut self, position: usize) {
        self.offset = position;
    }
}

impl<B> From<B> for Cursor<B> {
    #[inline]
    fn from(value: B) -> Self {
        Self::new(value)
    }
}
