mod cursor;
pub mod read;
pub mod write;

pub use self::{
    cursor::Cursor,
    read::{
        read,
        Read,
    },
};
use super::{
    buf::{
        chunks::NonEmptyIter,
        WriteError,
    },
    copy::CopyError,
    range::RangeOutOfBounds,
};
use crate::util::Peekable;

#[derive(Clone, Copy, Debug, Default, thiserror::Error)]
#[error("End of reader")]
pub struct End;

impl End {
    fn from_copy_error(e: CopyError) -> Self {
        match e {
            CopyError::SourceRangeOutOfBounds(_) => Self,
            _ => {
                panic!("Unexpected error while copying: {e}");
            }
        }
    }

    #[allow(dead_code)]
    fn from_range_out_of_bounds(_: RangeOutOfBounds) -> Self {
        // todo: we could do some checks here, if it's really an error that can be
        // interpreted as end of buffer.
        Self
    }
}

impl From<End> for std::io::ErrorKind {
    fn from(_: End) -> Self {
        std::io::ErrorKind::UnexpectedEof
    }
}

impl From<End> for std::io::Error {
    fn from(_: End) -> Self {
        std::io::ErrorKind::UnexpectedEof.into()
    }
}

#[derive(Clone, Copy, Debug, Default, thiserror::Error)]
#[error("Writer is full")]
pub struct Full;

impl Full {
    fn from_write_error(e: WriteError) -> Self {
        match e {
            WriteError::Full { .. } => Full,
            _ => panic!("Unexpected error while writing: {e}"),
        }
    }
}

/// A reader that knows how many bytes are remaining.
pub trait Remaining {
    fn remaining(&self) -> usize;

    #[inline]
    fn is_at_end(&self) -> bool {
        self.remaining() == 0
    }
}

/// A reader that also has knowledge about the position in the underlying
/// buffer.
pub trait Position {
    fn position(&self) -> usize;

    /// Set the position of the reader.
    ///
    /// It is up to the implementor how to handle invalid `position`s. The
    /// options are:
    ///
    /// 1. Panic immediately when [`set_position`](Self::set_position) is
    ///    called.
    /// 2. Ignore invalid positions until the [`Reader`] is being read from, and
    ///    then return [`End`].
    fn set_position(&mut self, position: usize);

    #[inline]
    fn is_at_start(&self) -> bool {
        self.position() == 0
    }

    #[inline]
    fn reset_position(&mut self) {
        self.set_position(0);
    }
}

/// A reader or writer that can skip bytes.
pub trait Skip {
    fn skip(&mut self, n: usize) -> Result<(), End>;
}

#[allow(dead_code)]
mod todo {
    use super::*;
    // todo: implement this. or do we even need this? don't forget to make this pub.

    pub struct ChunksReader<'a, I: Iterator<Item = &'a [u8]>> {
        inner: Peekable<NonEmptyIter<I>>,
    }

    impl<'a, I: Iterator<Item = &'a [u8]>> ChunksReader<'a, I> {
        #[inline]
        pub fn new(inner: I) -> Self {
            Self {
                inner: Peekable::new(NonEmptyIter(inner)),
            }
        }

        #[inline]
        pub fn into_parts(self) -> (I, Option<&'a [u8]>) {
            let (iter, peeked) = self.inner.into_parts();
            (iter.0, peeked)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::marker::PhantomData;

    use super::{
        read,
        Cursor,
        Read,
    };

    #[test]
    #[allow(dead_code)]
    fn derive_read_for_struct_of_basic_types() {
        #[derive(Read)]
        struct Foo {
            x1: u8,
            x2: i8,

            #[byst(big)]
            x3: u16,
            #[byst(little)]
            x4: u16,
            #[byst(big)]
            x5: i16,
            #[byst(little)]
            x6: i16,

            #[byst(big)]
            x7: u32,
            #[byst(little)]
            x8: u32,
            #[byst(big)]
            x9: i32,
            #[byst(little)]
            x10: i32,

            #[byst(big)]
            x11: u64,
            #[byst(little)]
            x12: u64,
            #[byst(big)]
            x13: i64,
            #[byst(little)]
            x14: i64,

            #[byst(big)]
            x15: u128,
            #[byst(little)]
            x16: u128,
            #[byst(big)]
            x17: i128,
            #[byst(little)]
            x18: i128,

            x19: (),
            x20: PhantomData<()>,
            x21: [u8; 4],
        }

        let mut cursor = Cursor::new(b"");
        let _ = read!(cursor => Foo);
    }

    #[test]
    #[allow(dead_code)]
    fn derive_read_for_nested_struct() {
        #[derive(Read)]
        struct Bar(u8);
        #[derive(Read)]
        struct Foo(Bar);

        let mut cursor = Cursor::new(b"");
        let _ = read!(cursor => Foo);
    }

    #[test]
    fn derive_read_uses_specified_endianness() {
        #[derive(Read)]
        struct Foo {
            #[byst(big)]
            x: u16,
            #[byst(little)]
            y: u16,
        }

        let mut cursor = Cursor::new(b"\x12\x34\x12\x34");
        let foo: Foo = read!(cursor).unwrap();

        assert_eq!(foo.x, 0x1234);
        assert_eq!(foo.y, 0x3412);
    }
}
