mod cursor;
pub mod read;
pub mod write;

use read::{
    End,
    ReadIntoBuf,
};

pub use self::{
    cursor::Cursor,
    read::{
        read,
        Read,
    },
};
use super::buf::chunks::NonEmpty;
use crate::{
    buf::{
        chunks::{
            ChunksExt,
            ChunksMutExt,
        },
        copy::copy_chunks,
    },
    util::Peekable,
    Buf,
    BufMut,
};

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

/// A reader that knows how many bytes are remaining.
pub trait Remaining {
    fn remaining(&self) -> usize;

    #[inline]
    fn is_at_end(&self) -> bool {
        self.remaining() == 0
    }
}

/// A reader or writer that can skip bytes.
pub trait Skip {
    fn skip(&mut self, n: usize) -> Result<(), End>;
}

pub struct ChunksReader<'a, B: Buf + 'a> {
    inner: Peekable<NonEmpty<B::Chunks<'a>>>,
    offset: usize,
}

impl<'a, B: Buf + 'a> ChunksReader<'a, B> {
    #[inline]
    pub fn new(inner: B::Chunks<'a>) -> Self {
        Self {
            inner: Peekable::new(inner.non_empty()),
            offset: 0,
        }
    }
}

impl<'a, B: Buf + 'a> ReadIntoBuf for ChunksReader<'a, B> {
    type Error = End;

    fn read_into_buf<D: BufMut>(&mut self, mut buf: D) -> Result<(), Self::Error> {
        let mut dest_chunks = Peekable::new(
            buf.chunks_mut(..)
                .map_err(End::from_range_out_of_bounds)?
                .non_empty(),
        );
        let mut dest_offset = 0;
        let total_copied = copy_chunks(
            &mut dest_chunks,
            &mut dest_offset,
            &mut self.inner,
            &mut self.offset,
            None,
        )
        .map_err(End::from_copy_error)?;
        drop(dest_chunks);
        if total_copied < buf.len() {
            Err(End)
        }
        else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::marker::PhantomData;

    use super::{
        read::{
            read,
            ReadIntoBuf,
        },
        ChunksReader,
        Cursor,
        Read,
    };
    use crate::{
        io::read::{
            End,
            InvalidDiscriminant,
        },
        Buf,
    };

    macro_rules! assert_derive_read {
        ($($ty:ty),*) => {
            {
                let mut cursor = Cursor::new(b"" as &'static [u8]);
                $(
                    match read!(&mut cursor => $ty) {
                        Ok(v) => {
                            let _: $ty = v;
                        }
                        Err(_) => {}
                    }
                )*
            }
        };
    }

    macro_rules! assert_read {
        ($ty:ty, $input:expr, $expected:expr) => {
            {
                let mut cursor = Cursor::new($input);
                let got = read!(&mut cursor => $ty).unwrap();
                assert_eq!(got, $expected);
            }
        };
        ($ty:ty, $input:expr, $expected:expr, $($arg:tt)+) => {
            {
                let mut cursor = Cursor::new($input);
                let got = read!(&mut cursor => $ty).unwrap();
                assert_eq!(got, $expected, $arg);
            }
        };
    }

    #[test]
    fn derive_read_for_unit_struct() {
        #[derive(Read)]
        struct Foo;
        #[derive(Read)]
        struct Bar();
        assert_derive_read!(Foo, Bar);
    }

    #[test]
    fn derive_read_for_struct_of_basic_types() {
        #[derive(Read)]
        #[allow(dead_code)]
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
        assert_derive_read!(Foo);
    }

    #[test]
    fn derive_read_for_nested_struct() {
        #[derive(Read)]
        #[allow(dead_code)]
        struct Bar(u8);
        #[derive(Read)]
        #[allow(dead_code)]
        struct Foo(Bar);
        assert_derive_read!(Foo);
    }

    #[test]
    fn derive_read_uses_specified_endianness() {
        #[derive(Read, Debug, PartialEq)]
        struct Foo {
            #[byst(big)]
            x: u16,
            #[byst(little)]
            y: u16,
            #[byst(network)]
            z: u16,
        }
        assert_read!(
            Foo,
            b"\x12\x34\x12\x34\x12\x34",
            Foo {
                x: 0x1234,
                y: 0x3412,
                z: 0x1234
            }
        );
    }

    #[test]
    fn derive_read_for_empty_enum() {
        #[derive(Debug, thiserror::Error)]
        #[error("oops")]
        enum MyErr {
            End(#[from] End),
            Invalid(#[from] InvalidDiscriminant<u8>),
        }

        #[derive(Read, Debug, PartialEq)]
        #[byst(discriminant(ty = "u8"), error = "MyErr")]
        enum Foo {}

        let mut cursor = Cursor::new(b"\x00\x00");
        let result = read!(&mut cursor => Foo);
        assert!(matches!(
            result,
            Err(MyErr::Invalid(InvalidDiscriminant(0)))
        ));
    }

    #[test]
    fn derive_read_for_simple_enum() {
        #[derive(Debug, thiserror::Error)]
        #[error("oops")]
        enum MyErr {
            End(#[from] End),
            Invalid(#[from] InvalidDiscriminant<u16>),
        }

        #[derive(Read, Debug, PartialEq)]
        #[byst(discriminant(ty = "u16", big), error = "MyErr")]
        enum Foo {
            One = 1,
            Two = 2,
        }

        assert_read!(Foo, b"\x00\x01", Foo::One);
        assert_read!(Foo, b"\x00\x02", Foo::Two);

        let mut cursor = Cursor::new(b"\x00\x03");
        let result = read!(&mut cursor => Foo);
        assert!(matches!(
            result,
            Err(MyErr::Invalid(InvalidDiscriminant(3)))
        ));
    }

    #[test]
    fn derive_read_for_enum_with_fields() {
        #[derive(Debug, thiserror::Error)]
        #[error("oops")]
        enum MyErr {
            End(#[from] End),
            Invalid(#[from] InvalidDiscriminant<u8>),
        }

        #[derive(Read, Debug, PartialEq)]
        #[byst(discriminant(ty = "u8"), error = "MyErr")]
        enum Foo {
            #[byst(discriminant = 1)]
            One {
                #[byst(big)]
                x: u16,
                #[byst(big)]
                y: u16,
            },
            #[byst(discriminant = 2)]
            Two(#[byst(big)] u16),
        }

        assert_read!(
            Foo,
            b"\x01\x01\x02\xab\xcd",
            Foo::One {
                x: 0x0102,
                y: 0xabcd
            }
        );
        assert_read!(Foo, b"\x02\xac\xab", Foo::Two(0xacab));
    }

    #[test]
    fn derive_read_for_enum_with_external_discriminant() {
        #[derive(Debug, thiserror::Error)]
        #[error("oops")]
        enum MyErr {
            End(#[from] End),
            Invalid(#[from] InvalidDiscriminant<u8>),
        }

        #[derive(Read, Debug, PartialEq)]
        #[byst(discriminant(ty = "u8"), params(name = "discriminant", ty = "u8"), match_expr = discriminant * 2, error = "MyErr")]
        enum Foo {
            #[byst(discriminant = 2)]
            One {
                #[byst(big)]
                x: u16,
                #[byst(big)]
                y: u16,
            },
            #[byst(discriminant = 4)]
            Two(#[byst(big)] u16),
        }

        #[derive(Read, Debug, PartialEq)]
        #[byst(error = "MyErr")]
        struct Bar {
            my_discriminant: u8,
            #[byst(big)]
            some_data: u16,
            #[byst(params(ty = "u8", with = my_discriminant))]
            foo: Foo,
        }

        assert_read!(
            Bar,
            b"\x01\x12\x34\x01\x02\xab\xcd",
            Bar {
                my_discriminant: 1,
                some_data: 0x1234,
                foo: Foo::One {
                    x: 0x0102,
                    y: 0xabcd
                }
            }
        );
        assert_read!(
            Bar,
            b"\x02\x12\x34\xac\xab",
            Bar {
                my_discriminant: 2,
                some_data: 0x1234,
                foo: Foo::Two(0xacab)
            }
        );
    }

    #[test]
    fn test_chunks_reader() {
        let buf = b"HelloWorld";
        let mut reader = ChunksReader::<&[u8]>::new(Buf::chunks(buf, ..).unwrap());

        let mut buf2 = [0u8; 5];

        reader.read_into_buf(&mut buf2).unwrap();
        assert_eq!(&buf2, b"Hello");

        reader.read_into_buf(&mut buf2).unwrap();
        assert_eq!(&buf2, b"World");

        match reader.read_into_buf(&mut buf2) {
            Err(End) => {}
            _ => panic!("Expected end of reader"),
        }
    }
}
