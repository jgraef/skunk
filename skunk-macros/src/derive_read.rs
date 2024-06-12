use darling::FromField;
use proc_macro2::TokenStream;
use proc_macro_error::abort;
use quote::quote;
use syn::{
    self,
    parse_quote,
    Data,
    DataStruct,
    DeriveInput,
};

use crate::{
    error::Error,
    options::{
        Bitfield,
        BitfieldFieldOptions,
        DeriveOptions,
        FieldOptions,
    },
    util::{
        field_name,
        make_where_clause,
    },
};

pub fn derive_read(item: DeriveInput, options: DeriveOptions) -> Result<TokenStream, Error> {
    let ident = &item.ident;
    if let Some(bitfield) = &options.bitfield {
        match &item.data {
            Data::Struct(s) => derive_read_for_struct_bitfield(s, bitfield, &item, &options),
            _ => abort!(ident, "Bitfields can only be derived on structs."),
        }
    }
    else {
        match &item.data {
            Data::Struct(s) => derive_read_for_struct(&s, &item, &options),
            Data::Enum(_) => todo!(),
            Data::Union(_) => abort!(ident, "Read can't be derive on unions."),
        }
    }
}

fn derive_read_for_struct(
    s: &DataStruct,
    item: &DeriveInput,
    _options: &DeriveOptions,
) -> Result<TokenStream, Error> {
    let ident = &item.ident;
    let (impl_generics, type_generics, where_clause) = item.generics.split_for_impl();
    let mut where_clause = make_where_clause(where_clause);
    let mut struct_init = Vec::with_capacity(s.fields.len());

    for (i, field) in s.fields.iter().enumerate() {
        let (_field_span, field_name) = field_name(i, field);
        let field_options = FieldOptions::from_field(&field)?;
        let field_ty = &field.ty;

        let read_field = if let Some(endianness) = field_options.endianness.ty() {
            where_clause.predicates.push(parse_quote! { #field_ty: for<'r> ::skunk::util::bytes::rw::ReadXe::<&'r mut __R, #endianness> });
            quote! {
                ::skunk::util::bytes::rw::ReadXe::<_, #endianness>::read(&mut reader)?
            }
        }
        else {
            where_clause.predicates.push(
                parse_quote! { #field_ty: for<'r> ::skunk::util::bytes::rw::Read::<&'r mut __R> },
            );
            quote! {
                ::skunk::util::bytes::rw::Read::<_>::read(&mut reader)?
            }
        };

        struct_init.push(quote! {
            #field_name: #read_field,
        });
    }

    Ok(quote! {
        #[automatically_derived]
        impl<__R, #impl_generics> ::skunk::util::bytes::rw::Read<__R> for #ident<#type_generics> #where_clause {
            fn read(mut reader: __R) -> ::std::result::Result<Self, ::skunk::util::bytes::rw::End> {
                ::std::result::Result::Ok(Self {
                    #(#struct_init)*
                })
            }
        }
    })
}

fn derive_read_for_struct_bitfield(
    s: &DataStruct,
    bitfield: &Bitfield,
    item: &DeriveInput,
    _options: &DeriveOptions,
) -> Result<TokenStream, Error> {
    let ident = &item.ident;
    let (impl_generics, type_generics, where_clause) = item.generics.split_for_impl();
    let mut where_clause = make_where_clause(where_clause);
    let bitfield_ty = &bitfield.ty;
    let mut struct_init = vec![];
    let mut bit_index = 0;

    let read_value = if let Some(endianness) = bitfield.endianness.ty() {
        where_clause.predicates.push(parse_quote! {
            #bitfield_ty: for<'r> ::skunk::util::bytes::rw::ReadXe::<&'r mut __R, #endianness>
        });
        quote! {
            ::skunk::util::bytes::rw::ReadXe::<_, #endianness>::read(&mut reader)?
        }
    }
    else {
        where_clause.predicates.push(parse_quote! {
            #bitfield_ty: for<'r> ::skunk::util::bytes::rw::Read::<&'r mut __R>
        });
        quote! {
            ::skunk::util::bytes::rw::Read::<_>::read(&mut reader)?
        }
    };

    for (i, field) in s.fields.iter().enumerate() {
        let (field_span, field_name) = field_name(i, field);
        let field_options = BitfieldFieldOptions::from_field(&field)?;
        let field_ty = &field.ty;

        let (start, bits) = match field_options {
            BitfieldFieldOptions {
                bits: Some(_),
                end: Some(_),
                ..
            } => {
                abort!(field_span, "Only one of `bits` and `end` can be specified")
            }

            BitfieldFieldOptions {
                bits: None,
                end: None,
                start,
                ..
            } => (start.unwrap_or(bit_index), 1),

            BitfieldFieldOptions {
                bits: Some(bits),
                start,
                end: None,
                ..
            } => (start.unwrap_or(bit_index), bits),

            BitfieldFieldOptions {
                bits: None,
                start,
                end: Some(end),
                ..
            } => {
                let start = start.unwrap_or(bit_index);
                (
                    start,
                    end.checked_sub(start)
                        .unwrap_or_else(|| abort!(field_span, "Bit field can't have end <= start")),
                )
            }
        };

        if bits == 0 {
            abort!(field_span, "Bit field can't be 0 bits");
        }

        where_clause.predicates.push(parse_quote! {
            #bitfield_ty: ::skunk::util::bytes::BitFieldExtract<#field_ty>
        });

        struct_init.push(quote!{
            #field_name: ::skunk::util::bytes::BitFieldExtract::extract::<#field_ty>::(#start, #bits),
        });

        bit_index = start + bits;
    }

    Ok(quote! {
        #[automatically_derived]
        impl<__R, #impl_generics> ::skunk::util::bytes::rw::Read<__R> for #ident<#type_generics> #where_clause {
            fn read(mut reader: __R) -> ::std::result::Result<Self, ::skunk::util::bytes::rw::End> {
                let _value: #bitfield_ty = #read_value;
                ::std::result::Result::Ok(Self {
                    #(#struct_init)*
                })
            }
        }
    })
}
