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
    Ident,
    Path,
};

use crate::{
    error::Error,
    options::{
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
    if let Some(bitfield_ty) = &options.bitfield {
        match &item.data {
            Data::Struct(s) => derive_read_for_struct_bitfield(s, bitfield_ty, &item, &options),
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

    let mut read_fields = Vec::with_capacity(s.fields.len());
    let mut struct_init = Vec::with_capacity(s.fields.len());

    for (i, field) in s.fields.iter().enumerate() {
        let (field_span, field_name) = field_name(i, field);
        let var = Ident::new(&format!("field_{i}"), field_span);
        let field_options = FieldOptions::from_field(&field)?;
        let field_ty = &field.ty;

        if let Some(endianness) = field_options.endianness() {
            read_fields.push(quote! {
                let #var = ::skunk::__private::rw::ReadXe::<_, #endianness>::read(&mut reader)?;
            });
            where_clause.predicates.push(parse_quote! { #field_ty: for<'r> ::skunk::__private::rw::ReadXe::<&'r mut __R, #endianness> });
        }
        else {
            read_fields.push(quote! {
                let #var = ::skunk::__private::rw::Read::<_>::read(&mut reader)?;
            });
            where_clause.predicates.push(
                parse_quote! { #field_ty: for<'r> ::skunk::__private::rw::Read::<&'r mut __R> },
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
    bitfield_ty: &Path,
    item: &DeriveInput,
    options: &DeriveOptions,
) -> Result<TokenStream, Error> {
    let ident = &item.ident;
    let (impl_generics, type_generics, where_clause) = item.generics.split_for_impl();
    let mut where_clause = make_where_clause(where_clause);

    where_clause.predicates.push(parse_quote! {
        #bitfield_ty: ::skunk::__private::rw::Read<__R>
    });
    where_clause.predicates.push(parse_quote! {
        ::skunk::__private::usize: ::skunk::__private::From<#bitfield_ty>
    });

    //let mut struct_init = vec![];

    Ok(quote! {
        #[automatically_derived]
        impl<__R, #impl_generics> ::skunk::__private::rw::Read<__R> for #ident<#type_generics> #where_clause {
            fn read(mut reader: __R) -> ::skunk::__private::Result<Self, ::skunk::__private::rw::End> {

                ::skunk::__private::Ok(Self {
                    //#(#struct_init)*
                })
            }
        }
    })
}
