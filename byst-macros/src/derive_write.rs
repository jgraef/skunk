use darling::{
    FromDeriveInput,
    FromField,
};
use proc_macro2::TokenStream;
use proc_macro_error::abort;
use quote::quote;
use syn::{
    parse_quote,
    Data,
    DataStruct,
    DeriveInput,
};

use crate::{
    error::Error,
    options::{
        Bitfield,
        FieldOptions,
        StructDeriveOptions,
    },
    util::{
        FieldName,
        SplitGenerics,
    },
};

pub fn derive_write(item: DeriveInput) -> Result<TokenStream, Error> {
    let ident = &item.ident;
    match &item.data {
        Data::Struct(s) => derive_write_for_struct(&s, &item),
        Data::Enum(_) => todo!(),
        Data::Union(_) => abort!(ident, "Write can't be derive on unions."),
    }
}

fn derive_write_for_struct(s: &DataStruct, item: &DeriveInput) -> Result<TokenStream, Error> {
    let _options = StructDeriveOptions::from_derive_input(&item)?;
    let ident = &item.ident;
    let SplitGenerics {
        mut impl_generics,
        type_generics,
        mut where_clause,
    } = SplitGenerics::from_generics(&item.generics);
    let mut write_fields = Vec::with_capacity(s.fields.len());

    for (i, field) in s.fields.iter().enumerate() {
        let FieldName {
            span: _,
            member: field_name,
            var: _,
        } = FieldName::from_field(i, field);
        let field_options = FieldOptions::from_field(&field)?;
        let field_ty = &field.ty;

        if let Some(endianness) = field_options.endianness.ty() {
            write_fields.push(quote! {
                ::byst::io::WriteXe::<_, #endianness>::write(&self.#field_name, &mut writer)?;
            });
            where_clause.predicates.push(
                parse_quote! { #field_ty: for<'w> ::byst::io::WriteXe::<&'w mut __W, #endianness> },
            );
        }
        else {
            write_fields.push(quote! {
                ::byst::io::Write::<_>::write(&self.#field_name, &mut writer)?;
            });
            where_clause
                .predicates
                .push(parse_quote! { #field_ty: for<'w> ::byst::io::Write::<&'w mut __W> });
        }
    }

    impl_generics.type_params.push(parse_quote! { __W });

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::byst::io::Write<__W> for #ident #type_generics #where_clause {
            fn write(&self, mut writer: __W) -> ::std::result::Result<(), ::byst::io::Full> {
                #(#write_fields)*
                ::std::result::Result::Ok(())
            }
        }
    })
}

fn derive_write_for_struct_bitfield(
    _s: &DataStruct,
    _bitfield: &Bitfield,
    _item: &DeriveInput,
) -> Result<TokenStream, Error> {
    todo!();
}
