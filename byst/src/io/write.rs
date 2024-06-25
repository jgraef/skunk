use std::{
    convert::Infallible,
    marker::PhantomData,
};

use byst_macros::for_tuple;

use super::Limit;
use crate::{
    buf::Buf,
    impl_me,
};

/// Something that can be written to a writer `W`, given the context `C`.
#[diagnostic::on_unimplemented(
    message = "The type `{Self}` cannot be be written to writer `{W}` with context `{C}`.",
    label = "Trying to write this",
    note = "Are you using the right context? Most integers for example need an endianness specified as context: e.g. `writer.write_with(123, NetworkEndian)`"
)]
pub trait Write<W: ?Sized, C> {
    type Error;

    fn write(&self, writer: &mut W, context: C) -> Result<(), Self::Error>;
}

pub trait Writer {
    type Error;

    fn write_buf<B: Buf>(&mut self, buf: B) -> Result<(), Self::Error>;
    fn skip(&mut self, amount: usize) -> Result<(), Self::Error>;
}

pub trait WriterExt: Writer {
    #[inline]
    fn write<T: Write<Self, ()>>(&mut self, value: &T) -> Result<(), T::Error> {
        Self::write_with(self, value, ())
    }

    #[inline]
    fn write_with<T: Write<Self, C>, C>(&mut self, value: &T, context: C) -> Result<(), T::Error> {
        T::write(value, self, context)
    }

    #[inline]
    fn limit(&mut self, limit: usize) -> Limit<&mut Self> {
        Limit::new(self, limit)
    }
}

impl<W: Writer> WriterExt for W {}

pub trait BufWriter: Writer {
    fn chunk_mut(&mut self) -> Option<&mut [u8]>;

    fn advance(&mut self, by: usize) -> Result<(), Full>;

    fn remaining(&self) -> usize;

    fn extend(&mut self, with: &[u8]) -> Result<(), Full>;
}

#[derive(Clone, Copy, Debug, Default, thiserror::Error)]
#[error(
    "Writer full: Tried to write {requested} bytes, but only {written} bytes could be written."
)]
pub struct Full {
    pub written: usize,
    pub requested: usize,
    pub remaining: usize,
}

impl From<Infallible> for Full {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}

impl From<crate::buf::Full> for Full {
    fn from(value: crate::buf::Full) -> Self {
        Self {
            written: 0,
            requested: value.required,
            remaining: value.capacity,
        }
    }
}

impl<'w, W: Writer> Writer for &'w mut W {
    type Error = W::Error;

    #[inline]
    fn write_buf<B: Buf>(&mut self, buf: B) -> Result<(), Self::Error> {
        <W as Writer>::write_buf(*self, buf)
    }

    #[inline]
    fn skip(&mut self, amount: usize) -> Result<(), Self::Error> {
        <W as Writer>::skip(*self, amount)
    }
}

impl_me! {
    impl['a] Writer for &'a mut [u8] as BufWriter;
    impl['a] Write<_, ()> for &'a [u8] as Writer::write_buf;
    //impl['a] Writer for &'a mut Vec<u8> as BufWriter;
}

impl<'a, W: BufWriter> BufWriter for &'a mut W {
    fn chunk_mut(&mut self) -> Option<&mut [u8]> {
        W::chunk_mut(self)
    }

    fn advance(&mut self, by: usize) -> Result<(), Full> {
        W::advance(self, by)
    }

    fn remaining(&self) -> usize {
        W::remaining(self)
    }

    fn extend(&mut self, with: &[u8]) -> Result<(), Full> {
        W::extend(self, with)
    }
}

impl<'a> BufWriter for &'a mut [u8] {
    #[inline]
    fn chunk_mut(&mut self) -> Option<&mut [u8]> {
        if self.is_empty() {
            None
        }
        else {
            Some(&mut **self)
        }
    }

    #[inline]
    fn advance(&mut self, by: usize) -> Result<(), Full> {
        if by <= self.len() {
            let (_, rest) = std::mem::take(self).split_at_mut(by);
            *self = rest;
            Ok(())
        }
        else {
            Err(Full {
                requested: by,
                remaining: self.len(),
                written: 0,
            })
        }
    }

    #[inline]
    fn remaining(&self) -> usize {
        self.len()
    }

    #[inline]
    fn extend(&mut self, with: &[u8]) -> Result<(), Full> {
        if with.len() <= self.len() {
            let (dest, rest) = std::mem::take(self).split_at_mut(with.len());
            dest.copy_from_slice(with);
            *self = rest;
            Ok(())
        }
        else {
            Err(Full {
                requested: with.len(),
                remaining: self.len(),
                written: 0,
            })
        }
    }
}

impl<W> Write<W, ()> for () {
    type Error = Infallible;

    #[inline]
    fn write(&self, _writer: &mut W, _context: ()) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<W, T> Write<W, ()> for PhantomData<T> {
    type Error = Infallible;

    #[inline]
    fn write(&self, _writer: &mut W, _context: ()) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<W: Writer, const N: usize> Write<W, ()> for [u8; N] {
    type Error = <W as Writer>::Error;

    #[inline]
    fn write(&self, writer: &mut W, _context: ()) -> Result<(), Self::Error> {
        writer.write_buf(self)
    }
}

impl<W: Writer> Write<W, ()> for u8 {
    type Error = <W as Writer>::Error;

    #[inline]
    fn write(&self, writer: &mut W, _context: ()) -> Result<(), Self::Error> {
        writer.write_buf(&[*self])
    }
}

impl<W: Writer> Write<W, ()> for i8 {
    type Error = <W as Writer>::Error;

    #[inline]
    fn write(&self, writer: &mut W, _context: ()) -> Result<(), Self::Error> {
        writer.write(&(*self as u8))
    }
}

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
        impl<W, $first_ty, $($tail_ty),*> Write<W, ()> for ($first_ty, $($tail_ty,)*)
        where
            $first_ty: Write<W, ()>,
            $($tail_ty: Write<W, (), Error = <$first_ty as Write<W, ()>>::Error>,)*
        {
            type Error = <$first_ty as Write<W, ()>>::Error;

            fn write(&self, mut writer: &mut W, _context: ()) -> Result<(), Self::Error> {
                <$first_ty as Write<W, ()>>::write(&self.$first_index, &mut writer, ())?;
                $(
                    <$tail_ty as Write<W, ()>>::write(&self.$tail_index, &mut writer, ())?;
                )*
                Ok(())
            }
        }
    };
}
for_tuple!(impl_read_for_tuple! for 1..=8);

#[cfg(test)]
mod tests {
    use std::marker::PhantomData;

    use crate::{
        buf::BufMut,
        io::{
            Write,
            WriterExt,
        },
    };

    macro_rules! assert_derive_write {
        ($($ty:ty),*) => {
            {
                let mut buf = vec![];
                let mut writer = buf.writer();
                $(
                    let _ = writer.write::<$ty>(&Default::default());
                )*
            }
        };
    }

    macro_rules! assert_write {
        ($input:expr, $expected:expr $(, $($arg:tt)+)?) => {
            {
                let mut buf = vec![];
                let mut writer = buf.writer();
                writer.write(&$input).expect("Expected write to be successful");
                assert_eq!(buf, $expected $(, $($arg)+)?);
            }
        };
    }

    #[test]
    fn derive_write_for_unit_struct() {
        #[derive(Write, Default)]
        struct Foo;
        #[derive(Write, Default)]
        struct Bar();
        #[derive(Write, Default)]
        struct Nya {}
        assert_derive_write!(Foo, Bar, Nya);
    }

    #[test]
    fn derive_write_for_struct_of_basic_types() {
        #[derive(Write, Default)]
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
        assert_derive_write!(Foo);
    }

    #[test]
    fn derive_write_for_nested_struct() {
        #[derive(Write, Default)]
        #[allow(dead_code)]
        struct Bar(u8);
        #[derive(Write, Default)]
        #[allow(dead_code)]
        struct Foo(Bar);
        assert_derive_write!(Foo);
    }

    #[test]
    fn derive_write_uses_specified_endianness() {
        #[derive(Write, Default, Debug, PartialEq)]
        struct Foo {
            #[byst(big)]
            x: u16,
            #[byst(little)]
            y: u16,
            #[byst(network)]
            z: u16,
        }
        assert_write!(
            Foo {
                x: 0x1234,
                y: 0x3412,
                z: 0x1234
            },
            b"\x12\x34\x12\x34\x12\x34"
        );
    }
}
