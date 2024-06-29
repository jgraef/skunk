use std::{
    convert::Infallible,
    marker::PhantomData,
    net::{
        Ipv4Addr,
        Ipv6Addr,
    },
};

use byst_macros::for_tuple;

use super::{
    Limit,
    Seek,
};
use crate::{
    impl_me,
    Buf,
    BufMut,
};

/// Something that can be read from a reader `R`, given the context `C`.
#[diagnostic::on_unimplemented(
    message = "The type `{Self}` cannot be be read from reader `{R}` with context `{C}`.",
    label = "Trying to read this",
    note = "Are you using the right context? Most integers for example need an endianness specified as context: e.g. `reader.read_with(NetworkEndian)`"
)]
pub trait Read<R: ?Sized, C>: Sized {
    type Error;

    fn read(reader: &mut R, context: C) -> Result<Self, Self::Error>;
}

pub trait Reader {
    type Error: ReadError;

    /// This may return succesful with less bytes that space is available in
    /// `dest` or `limit` specifies. It should only fail if the underlying data
    /// source fails, but needs to still provide a way to tell how many bytes
    /// have been read.
    fn read_into<D: BufMut>(
        &mut self,
        dest: D,
        limit: impl Into<Option<usize>>,
    ) -> Result<usize, Self::Error>;

    fn read_into_exact<D: BufMut>(&mut self, dest: D, length: usize) -> Result<(), Self::Error>;

    fn skip(&mut self, amount: usize) -> Result<(), Self::Error>;
}

pub trait ReadError {
    fn from_end(end: End) -> Self;

    fn is_end(&self) -> bool;

    fn amount_read(&self) -> usize;

    fn is_exact_end(&self) -> bool {
        self.is_end() && self.amount_read() == 0
    }
}

pub trait ReaderExt: Reader {
    #[inline]
    fn read<T: Read<Self, ()>>(&mut self) -> Result<T, T::Error> {
        self.read_with(())
    }

    #[inline]
    fn read_with<T: Read<Self, C>, C>(&mut self, context: C) -> Result<T, T::Error> {
        T::read(self, context)
    }

    #[inline]
    fn read_byte_array<const N: usize>(&mut self) -> Result<[u8; N], Self::Error> {
        let mut buf = [0u8; N];
        self.read_into_exact(&mut buf, N)?;
        Ok(buf)
    }

    #[inline]
    fn limit(&mut self, limit: usize) -> Limit<&mut Self> {
        Limit::new(self, limit)
    }
}

impl<R: Reader> ReaderExt for R {}

pub trait BufReader: Reader<Error = End> + Seek {
    type View: Buf;

    /// Returns a single chunk starting at the current position.
    ///
    /// This doesn't advance the cursor. You can use [`advance`][Self::advance]
    /// to advance the cursor.
    fn peek_chunk(&self) -> Option<&[u8]>;

    /// Returns a view of `length` bytes starting at the current position.
    ///
    /// This advances the cursor by `length` bytes.
    fn view(&mut self, length: usize) -> Result<Self::View, End>;

    /// Returns a view of `length` bytes starting at the current position.
    ///
    /// This doesn't advance the cursor. You can use [`advance`][Self::advance]
    /// to advance the cursor.
    fn peek_view(&self, length: usize) -> Result<Self::View, End>;

    /// Returns a view of the rest of this reader.
    ///
    /// This advances the cursor to the end of the reader.
    fn rest(&mut self) -> Self::View;

    /// Returns a view of the rest of this reader.
    ///
    /// This doesn't advance the cursor.
    fn peek_rest(&self) -> Self::View;

    /// Advances the cursor by `by` bytes.
    fn advance(&mut self, by: usize) -> Result<(), End>;

    /// Returns the number of bytes remaining.
    fn remaining(&self) -> usize;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, thiserror::Error)]
#[error("End of reader: Tried to read {requested} bytes, but only {read} could be read.")]
pub struct End {
    pub read: usize,
    pub requested: usize,
    pub remaining: usize,
}

impl ReadError for End {
    #[inline]
    fn from_end(end: End) -> Self {
        end
    }

    #[inline]
    fn amount_read(&self) -> usize {
        self.read
    }

    #[inline]
    fn is_end(&self) -> bool {
        true
    }
}

impl From<End> for std::io::ErrorKind {
    #[inline]
    fn from(_: End) -> Self {
        std::io::ErrorKind::UnexpectedEof
    }
}

impl From<End> for std::io::Error {
    #[inline]
    fn from(_: End) -> Self {
        std::io::ErrorKind::UnexpectedEof.into()
    }
}

impl From<Infallible> for End {
    #[inline]
    fn from(value: Infallible) -> Self {
        match value {}
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, thiserror::Error)]
#[error("Invalid discriminant: {0}")]
pub struct InvalidDiscriminant<D>(pub D);

impl<'a, R: Reader> Reader for &'a mut R {
    type Error = <R as Reader>::Error;

    #[inline]
    fn read_into<D: BufMut>(
        &mut self,
        dest: D,
        limit: impl Into<Option<usize>>,
    ) -> Result<usize, Self::Error> {
        <R as Reader>::read_into(*self, dest, limit)
    }

    #[inline]
    fn read_into_exact<D: BufMut>(&mut self, dest: D, length: usize) -> Result<(), Self::Error> {
        <R as Reader>::read_into_exact(*self, dest, length)
    }

    #[inline]
    fn skip(&mut self, amount: usize) -> Result<(), Self::Error> {
        <R as Reader>::skip(*self, amount)
    }
}

impl_me! {
    impl['a] Reader for &'a [u8] as BufReader;
    impl['a] Read<_, ()> for &'a [u8] as BufReader::View;
}

impl<'a, R: BufReader> BufReader for &'a mut R {
    type View = R::View;

    #[inline]
    fn peek_chunk(&self) -> Option<&[u8]> {
        R::peek_chunk(*self)
    }

    #[inline]
    fn view(&mut self, length: usize) -> Result<Self::View, End> {
        R::view(*self, length)
    }

    #[inline]
    fn peek_view(&self, length: usize) -> Result<Self::View, End> {
        R::peek_view(*self, length)
    }

    #[inline]
    fn rest(&mut self) -> Self::View {
        R::rest(*self)
    }

    #[inline]
    fn peek_rest(&self) -> Self::View {
        R::peek_rest(*self)
    }

    #[inline]
    fn advance(&mut self, by: usize) -> Result<(), End> {
        R::advance(*self, by)
    }

    #[inline]
    fn remaining(&self) -> usize {
        R::remaining(*self)
    }
}

impl<'a> BufReader for &'a [u8] {
    type View = &'a [u8];

    #[inline]
    fn peek_chunk(&self) -> Option<&[u8]> {
        if self.is_empty() {
            None
        }
        else {
            Some(*self)
        }
    }

    #[inline]
    fn view(&mut self, length: usize) -> Result<Self::View, End> {
        if length <= self.len() {
            let (left, right) = self.split_at(length);
            *self = right;
            Ok(left)
        }
        else {
            Err(End {
                requested: length,
                read: 0,
                remaining: self.len(),
            })
        }
    }

    #[inline]
    fn peek_view(&self, length: usize) -> Result<Self::View, End> {
        if length <= self.len() {
            Ok(&self[..length])
        }
        else {
            Err(End {
                requested: length,
                read: 0,
                remaining: self.len(),
            })
        }
    }

    #[inline]
    fn rest(&mut self) -> Self::View {
        std::mem::take(self)
    }

    #[inline]
    fn peek_rest(&self) -> Self::View {
        self
    }

    #[inline]
    fn advance(&mut self, by: usize) -> Result<(), End> {
        if by <= self.len() {
            *self = &self[by..];
            Ok(())
        }
        else {
            Err(End {
                read: 0,
                requested: by,
                remaining: self.len(),
            })
        }
    }

    #[inline]
    fn remaining(&self) -> usize {
        self.len()
    }
}

impl<R> Read<R, ()> for () {
    type Error = Infallible;

    #[inline]
    fn read(_reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        Ok(())
    }
}

impl<R, T> Read<R, ()> for PhantomData<T> {
    type Error = Infallible;

    #[inline]
    fn read(_reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        Ok(PhantomData)
    }
}
/*
impl<R: Reader, C, T: Read<R, C>, const N: usize> Read<R, C> for [T; N] {
    type Error = End;

    #[inline]
    fn read(reader: &mut R, _context: C) -> Result<Self, Self::Error> {
        todo!();
    }
}
*/

impl<R: Reader, const N: usize> Read<R, ()> for [u8; N] {
    type Error = <R as Reader>::Error;

    #[inline]
    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        reader.read_byte_array()
    }
}

impl<R: Reader> Read<R, ()> for u8 {
    type Error = <R as Reader>::Error;

    #[inline]
    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        Ok(reader.read_byte_array::<1>()?[0])
    }
}

impl<R: Reader> Read<R, ()> for i8 {
    type Error = <R as Reader>::Error;

    #[inline]
    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        Ok(reader.read::<u8>()? as i8)
    }
}

impl<R: Reader> Read<R, ()> for Ipv4Addr {
    type Error = <R as Reader>::Error;

    #[inline]
    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        Ok(Ipv4Addr::from(reader.read::<[u8; 4]>()?))
    }
}

impl<R: Reader> Read<R, ()> for Ipv6Addr {
    type Error = <R as Reader>::Error;

    #[inline]
    fn read(reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
        Ok(Ipv6Addr::from(reader.read::<[u8; 16]>()?))
    }
}

/// Implements [`Read`] for tuples.
///
/// # TODO
///
/// - add params
macro_rules! impl_read_for_tuple {
    (
        $index:tt => $name:ident: $ty:ident
    ) => {
        impl_read_for_tuple! {
            $index => $name: $ty,
        }
    };
    (
        $first_index:tt => $first_name:ident: $first_ty:ident,
        $($tail_index:tt => $tail_name:ident: $tail_ty:ident),*
    ) => {
        impl<R, $first_ty, $($tail_ty),*> Read<R, ()> for ($first_ty, $($tail_ty,)*)
        where
            $first_ty: Read<R, ()>,
            $($tail_ty: Read<R, (), Error = <$first_ty as Read<R, ()>>::Error>,)*
        {
            type Error = <$first_ty as Read<R, ()>>::Error;

            fn read(mut reader: &mut R, _context: ()) -> Result<Self, Self::Error> {
                let $first_name = <$first_ty as Read<R, ()>>::read(&mut reader, ())?;
                $(
                    let $tail_name = <$tail_ty as Read<R, ()>>::read(&mut reader, ())?;
                )*
                Ok((
                    $first_name,
                    $($tail_name,)*
                ))
            }
        }
    };
}
for_tuple!(impl_read_for_tuple! for 1..=8);

/// Read macro
///
/// # TODO
///
/// - deprecate this. `reader.read::<T>()` and `reader.read_with::<T,
///   _>(context)` are nicer.
#[macro_export]
macro_rules! read {
    ($reader:expr => $ty:ty; $params:expr) => {
        {
            <$ty as ::byst::io::Read::<_, _>>::read($reader, $params)
        }
    };
    ($reader:expr => $ty:ty) => {
        read!($reader => $ty; ())
    };
    ($reader:expr; $params:expr) => {
        read!($reader => _; $params)
    };
    ($reader:expr) => {
        read!($reader => _; ())
    };
}
pub use read;

#[cfg(test)]
mod tests {
    use std::marker::PhantomData;

    use crate::io::{
        read,
        End,
        InvalidDiscriminant,
        Read,
        ReaderExt,
    };

    macro_rules! assert_derive_read {
        ($($ty:ty),*) => {
            {
                let mut reader: &'static [u8] = b"";
                $(
                    match reader.read::<$ty>() {
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
        ($ty:ty, $input:expr, $expected:expr $(, $($arg:tt)+)?) => {
            {
                let mut reader: &'static [u8] = $input;
                let got = reader.read::<$ty>().expect("Expected read to be successful");
                assert_eq!(got, $expected $(, $($arg)+)?);
            }
        };
    }

    macro_rules! assert_read_fail {
        ($ty:ty, $input:expr, $expected:expr $(, $($arg:tt)+)?) => {
            {
                let mut reader: &'static [u8] = $input;
                let got = reader.read::<$ty>().expect_err("Expected read to fail");
                assert_eq!(got, $expected $(, $($arg)+)?);
            }
        };
    }

    #[test]
    fn derive_read_for_unit_struct() {
        #[derive(Read)]
        struct Foo;
        #[derive(Read)]
        struct Bar();
        #[derive(Read)]
        struct Nya {}
        assert_derive_read!(Foo, Bar, Nya);
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
        #[derive(Debug, PartialEq, Eq, thiserror::Error)]
        #[error("oops")]
        enum MyErr {
            Incomplete(#[from] End),
            Invalid(#[from] InvalidDiscriminant<u8>),
        }

        #[derive(Read, Debug, PartialEq)]
        #[byst(tag(ty = "u8"), error = "MyErr")]
        enum Foo {}

        let mut reader: &'static [u8] = b"\x00\x00";
        let result = read!(&mut reader => Foo);
        assert!(matches!(
            result,
            Err(MyErr::Invalid(InvalidDiscriminant(0)))
        ));
    }

    #[test]
    fn derive_read_for_simple_enum() {
        #[derive(Debug, PartialEq, Eq, thiserror::Error)]
        #[error("oops")]
        enum MyErr {
            Incomplete(#[from] End),
            Invalid(#[from] InvalidDiscriminant<u16>),
        }

        #[derive(Read, Debug, PartialEq)]
        #[byst(tag(ty = "u16", big), error = "MyErr")]
        enum Foo {
            One = 1,
            Two = 2,
        }

        assert_read!(Foo, b"\x00\x01", Foo::One);
        assert_read!(Foo, b"\x00\x02", Foo::Two);
        assert_read_fail!(Foo, b"\x00\x03", MyErr::Invalid(InvalidDiscriminant(3)));
    }

    #[test]
    fn derive_read_for_enum_with_fields() {
        #[derive(Debug, PartialEq, Eq, thiserror::Error)]
        #[error("oops")]
        enum MyErr {
            Incomplete(#[from] End),
            Invalid(#[from] InvalidDiscriminant<u8>),
        }

        #[derive(Read, Debug, PartialEq)]
        #[byst(tag(ty = "u8"), error = "MyErr")]
        enum Foo {
            #[byst(tag = 1)]
            One {
                #[byst(big)]
                x: u16,
                #[byst(big)]
                y: u16,
            },
            #[byst(tag = 2)]
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
        #[derive(Debug, PartialEq, Eq, thiserror::Error)]
        #[error("oops")]
        enum MyErr {
            End(#[from] End),
            Invalid(#[from] InvalidDiscriminant<u8>),
        }

        #[derive(Read, Debug, PartialEq)]
        #[byst(tag(ty = "u8"), context(name = "discriminant", ty = "u8"), match_expr = discriminant * 2, error = "MyErr")]
        enum Foo {
            #[byst(tag = 2)]
            One {
                #[byst(big)]
                x: u16,
                #[byst(big)]
                y: u16,
            },
            #[byst(tag = 4)]
            Two(#[byst(big)] u16),
        }

        #[derive(Read, Debug, PartialEq)]
        #[byst(error = "MyErr")]
        struct Bar {
            my_tag: u8,
            #[byst(big)]
            some_data: u16,
            #[byst(context(ty = "u8", with = my_tag))]
            foo: Foo,
        }

        assert_read!(
            Bar,
            b"\x01\x12\x34\x01\x02\xab\xcd",
            Bar {
                my_tag: 1,
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
                my_tag: 2,
                some_data: 0x1234,
                foo: Foo::Two(0xacab)
            }
        );
    }
}
