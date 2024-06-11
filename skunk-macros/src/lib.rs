use darling::{
    FromAttributes,
    FromDeriveInput,
};
use proc_macro2::{
    Span,
    TokenStream,
};
use proc_macro_error::{
    abort,
    abort_call_site,
    proc_macro_error,
};
use quote::{
    quote,
    quote_spanned,
};
use syn::{
    parse::{
        Parse,
        ParseStream,
    },
    parse_macro_input,
    punctuated::Punctuated,
    spanned::Spanned,
    token::{
        Colon,
        Where,
    },
    Data,
    DataStruct,
    DeriveInput,
    Field,
    Ident,
    Index,
    ItemStruct,
    LitInt,
    Path,
    PredicateType,
    Token,
    Type,
    TypeParamBound,
    WhereClause,
    WherePredicate,
};

#[proc_macro_error]
#[proc_macro_derive(Read, attributes(skunk))]
pub fn derive_read_macro(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    derive_helper(input, derive_read)
}

#[proc_macro_error]
#[proc_macro_derive(Write, attributes(skunk))]
pub fn derive_write_macro(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    derive_helper(input, derive_write)
}

fn derive_helper(
    input: proc_macro::TokenStream,
    deriver: impl FnOnce(DeriveInput, DeriveOptions) -> Result<TokenStream, Error>,
) -> proc_macro::TokenStream {
    let item = parse_macro_input!(input as DeriveInput);

    let options = match DeriveOptions::from_derive_input(&item) {
        Ok(options) => options,
        Err(e) => {
            return Error::from(e).write_errors().into();
        }
    };

    match deriver(item, options) {
        Ok(output) => output.into(),
        Err(e) => e.write_errors().into(),
    }
}

#[derive(FromDeriveInput)]
#[darling(attributes(skunk), forward_attrs(allow, doc, cfg))]
struct DeriveOptions {
    //ident: Ident,
    //attrs: Vec<Attribute>,
    //endianness: Option<Path>,
    #[darling(default)]
    bitfield: bool,
}

#[derive(FromAttributes)]
#[darling(attributes(skunk))]
struct FieldOptions {
    endianness: Option<Path>,
    #[darling(default)]
    big: bool,
    #[darling(default)]
    little: bool,
    #[darling(default)]
    network: bool,
    #[darling(default)]
    native: bool,
}

impl FieldOptions {
    pub fn endianness(&self) -> Option<TokenStream> {
        match (
            self.big,
            self.little,
            self.network,
            self.native,
            &self.endianness,
        ) {
            (false, false, false, false, None) => None,
            (true, false, false, false, None) => Some(quote! { ::skunk::__private::rw::BigEndian }),
            (false, true, false, false, None) => {
                Some(quote! { ::skunk::__private::rw::LittleEndian })
            }
            (false, false, true, false, None) => {
                Some(quote! { ::skunk::__private::rw::NetworkEndian })
            }
            (false, false, false, true, None) => {
                Some(quote! { ::skunk::__private::rw::NativeEndian })
            }
            (false, false, false, false, Some(path)) => Some(quote! { #path }),
            _ => {
                abort_call_site!(
                    "Only one of `big`, `little`, `network`, `native`, or `endianness = PATH` may be specified."
                )
            }
        }
    }
}

/// Derive [`Read`][1] implementation.
///
/// # Example
///
/// Deriving [`Read`][1] on a struct will generate an implementation that reads
/// the fields in the order they appear in the declaration.
///
/// ```
/// # use skunk_macros::Read;
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
///     // You can also specify the specific type for the endianness.
///     #[skunk(endianness = "NetworkEndian")]
///     y: i64,
///
///     // Can contain other stucts that can be read.
///     bar: Bar,
/// }
/// ```
///
/// [1]: skunk::util::bytes::rw::Read
fn derive_read(item: DeriveInput, options: DeriveOptions) -> Result<TokenStream, Error> {
    let ident = &item.ident;
    if options.bitfield {
        match &item.data {
            Data::Struct(s) => derive_read_for_struct_bitfield(s, &item, options),
            _ => abort!(ident, "Bitfields can only be derived on structs."),
        }
    }
    else {
        match &item.data {
            Data::Struct(s) => derive_read_for_struct(&s, &item, options),
            Data::Enum(_) => todo!(),
            Data::Union(_) => abort!(ident, "Read can't be derive on unions."),
        }
    }
}

fn derive_read_for_struct(
    s: &DataStruct,
    item: &DeriveInput,
    _options: DeriveOptions,
) -> Result<TokenStream, Error> {
    let ident = &item.ident;
    let (impl_generics, type_generics, where_clause) = item.generics.split_for_impl();
    let mut where_clause = make_where_clause(where_clause);

    let mut read_fields = vec![];
    let mut struct_init = vec![];

    for (i, field) in s.fields.iter().enumerate() {
        let (field_span, field_name) = field_name(i, field);
        let var = Ident::new(&format!("field_{i}"), field_span);
        let attrs = FieldOptions::from_attributes(&field.attrs)?;

        if let Some(endianness) = attrs.endianness() {
            read_fields.push(quote! {
                let #var = ::skunk::__private::rw::ReadXe::<_, #endianness>::read(&mut reader)?;
            });
            add_trait_bound(
                &mut where_clause,
                &field.ty,
                quote! {
                    for<'r> ::skunk::__private::rw::ReadXe::<&'r mut __R, #endianness>
                },
            );
        }
        else {
            read_fields.push(quote! {
                let #var = ::skunk::__private::rw::Read::<_>::read(&mut reader)?;
            });
            add_trait_bound(
                &mut where_clause,
                &field.ty,
                quote! {
                    for<'r> ::skunk::__private::rw::Read::<&'r mut __R>
                },
            );
        }

        struct_init.push(quote! {
            #field_name: #var,
        });
    }

    Ok(quote! {
        #[automatically_derived]
        impl<__R, #impl_generics> ::skunk::__private::rw::Read<__R> for #ident<#type_generics> #where_clause {
            fn read(mut reader: __R) -> ::skunk::__private::Result<Self, ::skunk::__private::rw::End> {
                #(#read_fields)*
                ::skunk::__private::Ok(Self {
                    #(#struct_init)*
                })
            }
        }
    })
}

fn derive_read_for_struct_bitfield(
    s: &DataStruct,
    item: &DeriveInput,
    options: DeriveOptions,
) -> Result<TokenStream, Error> {
    todo!()
}

fn derive_write(item: DeriveInput, _options: DeriveOptions) -> Result<TokenStream, Error> {
    let ident = &item.ident;
    let (impl_generics, type_generics, where_clause) = item.generics.split_for_impl();
    let mut where_clause = make_where_clause(where_clause);

    match item.data {
        Data::Struct(s) => {
            let mut write_fields = vec![];

            for (i, field) in s.fields.iter().enumerate() {
                let (_, field_name) = field_name(i, field);
                let attrs = FieldOptions::from_attributes(&field.attrs)?;

                if let Some(endianness) = attrs.endianness() {
                    write_fields.push(quote! {
                        ::skunk::__private::rw::WriteXe::<_, #endianness>::write(&self.#field_name, &mut writer)?;
                    });
                    add_trait_bound(
                        &mut where_clause,
                        &field.ty,
                        quote! {
                            for<'w> ::skunk::__private::rw::WriteXe::<&'w mut __W, #endianness>
                        },
                    );
                }
                else {
                    write_fields.push(quote! {
                        ::skunk::__private::rw::Write::<_>::write(&self.#field_name, &mut writer)?;
                    });
                    add_trait_bound(
                        &mut where_clause,
                        &field.ty,
                        quote! {
                            for<'w> ::skunk::__private::rw::Write::<&'w mut __W>
                        },
                    );
                }
            }

            Ok(quote! {
                #[automatically_derived]
                impl<__W, #impl_generics> ::skunk::__private::rw::Write<__W> for #ident<#type_generics> #where_clause {
                    fn write(&self, mut writer: __W) -> ::skunk::__private::Result<(), ::skunk::__private::rw::Full> {
                        #(#write_fields)*
                        ::skunk::__private::Ok(())
                    }
                }
            })
        }
        Data::Enum(_) => todo!(),
        Data::Union(_) => abort!(ident, "Can't derive Read for union"),
    }
}

#[derive(Debug)]
struct ForTupleInput {
    callback: Path,
    min: usize,
    max: usize,
}

impl Parse for ForTupleInput {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let callback: Path = input.parse()?;

        input.parse::<Token![!]>()?;
        input.parse::<Token![for]>()?;

        let min = input
            .parse::<Option<LitInt>>()?
            .map(|n| n.base10_parse::<usize>())
            .transpose()?
            .unwrap_or_default();

        let dotdoteq = input.parse::<Option<Token![..=]>>()?;
        let to_inclusive = if dotdoteq.is_some() {
            true
        }
        else {
            if input.parse::<Option<Token![..]>>()?.is_none() {
                return Err(input.error("Expected either `..` or `..=`"));
            }
            false
        };

        let max = input.parse::<LitInt>()?.base10_parse::<usize>()?
            + to_inclusive.then_some(1).unwrap_or_default();

        Ok(Self { callback, min, max })
    }
}

/// Calls another macro with tuples of specified lengths.
///
/// This is useful to implement for tuple types.
///
/// The callback macro will be passed a comma-separated list of
///
/// ```
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
/// # use skunk_macros::for_tuple;
/// # trait Foo { fn foo(&self); }
/// macro_rules! impl_tuple {
///     ($($index:tt => $name:ident : $ty:ident),*) => {
///         impl Foo for ($($ty,)) {
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
    let callback = &input.callback;

    let mut indices = Vec::with_capacity(input.max);
    let mut names = Vec::with_capacity(input.max);
    let mut tys = Vec::with_capacity(input.max);
    for i in 0..input.max {
        indices.push(Index {
            index: i as u32,
            span: Span::call_site(),
        });
        names.push(Ident::new(&format!("_{}", i + 1), Span::call_site()));
        tys.push(Ident::new(&format!("T{}", i + 1), Span::call_site()));
    }
    let mut output = vec![];

    for i in input.min..input.max {
        let index = &indices[..i];
        let name = &names[..i];
        let ty = &tys[..i];

        output.push(quote! {
            #callback!(#(#index => #name: #ty),*);
        });
    }

    quote! {
        #(#output)*
    }
    .into()
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("syn error")]
    Syn(#[from] syn::Error),

    #[error("darling error")]
    Darling(#[from] darling::Error),
}

impl Error {
    pub fn write_errors(self) -> TokenStream {
        match self {
            Error::Syn(e) => e.into_compile_error(),
            Error::Darling(e) => e.write_errors(),
        }
    }
}

fn field_name(index: usize, field: &Field) -> (Span, TokenStream) {
    field.ident.as_ref().map_or_else(
        || {
            let span = field.ty.span();
            let index = Index {
                index: index as u32,
                span,
            };
            (span, quote_spanned! { field.ty.span() => #index })
        },
        |ident| (ident.span(), quote! { #ident }),
    )
}

fn make_where_clause(where_clause: Option<&WhereClause>) -> WhereClause {
    where_clause.cloned().unwrap_or_else(|| {
        WhereClause {
            where_token: Where {
                span: Span::call_site(),
            },
            predicates: Punctuated::new(),
        }
    })
}

fn add_trait_bound(where_clause: &mut WhereClause, ty: &Type, bound: TokenStream) {
    where_clause
        .predicates
        .push(WherePredicate::Type(PredicateType {
            lifetimes: None,
            bounded_ty: ty.clone(),
            colon_token: Colon {
                spans: [Span::call_site()],
            },
            bounds: [TypeParamBound::Verbatim(bound)].into_iter().collect(),
        }));
}
