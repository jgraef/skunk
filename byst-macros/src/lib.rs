//! Macros for the `byst` crate.

#![allow(dead_code, rustdoc::broken_intra_doc_links)]
mod bitmask;
mod derive_read;
mod derive_write;
mod error;
mod for_tuple;
mod options;
mod util;

use proc_macro_error::proc_macro_error;
use syn::parse_macro_input;

use crate::{
    bitmask::BitRangeInput,
    for_tuple::ForTupleInput,
    util::derive_helper,
};

/// Derive [`Read`][1] implementation.
///
/// # Example
///
/// Deriving [`Read`][1] on a struct will generate an implementation that reads
/// the fields in the order they appear in the declaration.
///
/// ```ignore
/// # use byst_macros::Read;
/// # type NetworkEndian = ();
/// # type Bar = ();
///
/// #[derive(Read)]
/// struct Foo {
///     // Can contain integers.
///     //
///     // For anything other than `u8` and `i8` you need to specify the endianness. Valid attributes are `little`, `big`, `network`, `native`.
///     #[skunk(little)]
///     x: u32,
///
///     // Can contain other stucts that can be read.
///     bar: Bar,
/// }
/// ```
///
/// [1]: skunk::util::bytes::rw::Read
#[proc_macro_error]
#[proc_macro_derive(Read, attributes(byst))]
pub fn derive_read(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    derive_helper(input, crate::derive_read::derive_read)
}

#[proc_macro_error]
#[proc_macro_derive(Write, attributes(byst))]
pub fn derive_write(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    derive_helper(input, crate::derive_write::derive_write)
}

/// Calls another macro with tuples of specified lengths.
///
/// This is useful to implement for tuple types.
///
/// The callback macro will be passed a comma-separated list of
///
/// ```no_rust
/// $index:tt => $name:ident : $ty:ident
/// ```
///
/// where:
///
/// - `$index` is the index for that entry, to be used for field access.
/// - `$name` is a generated identifier of form `_$index`, to be used as
///   variables.
/// - `$ty` is the type of that entry.
///
/// # Example
///
/// ```
/// # use byst_macros::for_tuple;
/// # trait Foo { fn foo(&self); }
/// macro_rules! impl_tuple {
///     ($($index:tt => $name:ident : $ty:ident),*) => {
///         impl<$($ty),*> Foo for ($($ty,)*) {
///             fn foo(&self) {
///                 $(
///                     let $name = &self.$index;
///                 )*
///                 // do something with variables
///             }
///         }
///         // do stuff here
///     }
/// }
///
/// // Implements Foo for tuples of size 1 to 8 (inclusive).
/// for_tuple!(impl_tuple! for 1..=8);
/// ```
///
/// Note that the `impl` is on the type `($($ty,))` with the comma inside, such
/// that `(T,)` is a tuple.
#[proc_macro]
pub fn for_tuple(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as ForTupleInput);
    match crate::for_tuple::for_tuple(input) {
        Ok(output) => output.into(),
        Err(e) => e.write_errors().into(),
    }
}

/// Generates a bit mask for the given range. The output will be an integer
/// literal.
///
/// # Example
///
/// ```
/// # use byst_macros::bit_range;
/// let x = bit_range!(4..=8);
/// ```
#[proc_macro]
pub fn bit_range(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as BitRangeInput);
    match crate::bitmask::bit_range(input) {
        Ok(output) => output.into(),
        Err(e) => e.write_errors().into(),
    }
}
