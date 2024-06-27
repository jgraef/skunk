use byst::{
    io::{
        BufReader,
        Writer,
    },
    Buf,
};
use crc::{
    Crc,
    Digest,
    Width,
};

pub trait CrcExt {
    type Output;

    fn for_reader(&'static self, reader: impl BufReader) -> Self::Output;

    #[inline]
    fn for_buf(&'static self, buf: impl Buf) -> Self::Output {
        self.for_reader(buf.reader())
    }
}

impl<W: CrcInt> CrcExt for crc::Crc<W> {
    type Output = W;

    fn for_reader(&self, mut reader: impl BufReader) -> Self::Output {
        let mut digest = W::crc_digest(self);
        while let Some(chunk) = reader.chunk() {
            W::digest_update(&mut digest, chunk);
            reader.advance(chunk.len()).unwrap();
        }
        W::digest_finalize(digest)
    }
}

pub struct CrcWriter<'c, C: Width, W> {
    writer: W,
    digest: Digest<'c, C>,
}

impl<'c, C: CrcInt, W> CrcWriter<'c, C, W> {
    pub fn new(crc: &'c Crc<C>, writer: W) -> Self {
        let digest = C::crc_digest(crc);
        Self { writer, digest }
    }

    pub fn finalize(self) -> (W, C) {
        (self.writer, C::digest_finalize(self.digest))
    }
}

impl<'c, C: CrcInt, W: Writer> Writer for CrcWriter<'c, C, W> {
    type Error = W::Error;

    fn write_buf<B: Buf>(&mut self, buf: B) -> Result<(), Self::Error> {
        let mut reader = buf.reader();
        while let Some(chunk) = reader.chunk() {
            C::digest_update(&mut self.digest, chunk);
            self.writer.write_buf(chunk)?;
            reader.advance(chunk.len()).unwrap();
        }
        Ok(())
    }

    fn skip(&mut self, amount: usize) -> Result<(), Self::Error> {
        // todo: what should we do here?
        self.writer.skip(amount)
    }
}

pub trait CrcInt: Width {
    fn crc_digest<'a>(crc: &'a Crc<Self>) -> Digest<'a, Self>;
    fn digest_update<'a>(digest: &mut Digest<'a, Self>, data: &[u8]);
    fn digest_finalize<'a>(digest: Digest<'a, Self>) -> Self;
}

macro_rules! impl_crc_int {
    ($($ty:ty),*) => {
        $(
            impl CrcInt for $ty {
                fn crc_digest<'a>(crc: &'a Crc<Self>) -> Digest<'a, Self> {
                    crc.digest()
                }

                fn digest_update<'a>(digest: &mut Digest<'a, Self>, data: &[u8]) {
                    digest.update(data);
                }

                fn digest_finalize<'a>(digest: Digest<'a, Self>) -> Self {
                    digest.finalize()
                }
            }
        )*
    };
}

impl_crc_int!(u8, u16, u32, u64, u128);
