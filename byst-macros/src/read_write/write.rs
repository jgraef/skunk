use darling::FromField;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    parse_quote,
    Data,
    DataStruct,
    DeriveInput,
    Fields,
};

use super::{
    Bitfield,
    DeriveBounds,
    DeriveOptions,
    FieldOptions,
};
use crate::{
    error::Error,
    util::{
        Deriver,
        FieldName,
        SplitGenerics,
    },
};

pub struct DeriveWrite;

impl Deriver for DeriveWrite {
    const NAME: &'static str = "Write";
    type Options = DeriveOptions;

    fn derive_for_item(item: DeriveInput, options: Self::Options) -> Result<TokenStream, Error> {
        if options.transparent {
            derive_transparent(item, options)
        }
        else {
            match &item.data {
                Data::Struct(s) => Self::derive_for_struct(&s, &item, options),
                Data::Enum(e) => Self::derive_for_enum(&e, &item, options),
                Data::Union(u) => Self::derive_for_union(&u, &item, options),
            }
        }
    }

    fn derive_for_struct(
        data: &DataStruct,
        item: &DeriveInput,
        options: Self::Options,
    ) -> Result<TokenStream, Error> {
        options.check_for_struct();

        let ident = &item.ident;
        let (context_name, context_ty) = options.context();

        let SplitGenerics {
            mut impl_generics,
            type_generics,
            where_clause,
        } = SplitGenerics::from_generics(&item.generics);
        impl_generics.type_params.push(parse_quote! { __W });
        let mut bounds = DeriveBounds::new(where_clause, options.error.clone());

        let write_fields = make_struct_write_fields(&data.fields, &mut bounds)?;

        let (where_clause, error_ty) = bounds.finish();

        Ok(quote! {
            #[automatically_derived]
            impl #impl_generics ::byst::io::Write<__W, #context_ty> for #ident #type_generics #where_clause {
                type Error = #error_ty;

                fn write(&self, mut __writer: &mut __W, #context_name: #context_ty) -> ::std::result::Result<(), Self::Error> {
                    #write_fields
                    ::std::result::Result::Ok(())
                }
            }
        })
    }
}

fn make_struct_write_fields(
    fields: &Fields,
    bounds: &mut DeriveBounds,
) -> Result<TokenStream, Error> {
    let mut write_fields = Vec::with_capacity(fields.len());

    for (i, field) in fields.iter().enumerate() {
        let FieldName {
            span: _,
            member: field_name,
            var: _,
        } = FieldName::from_field(i, field);
        let field_options = FieldOptions::from_field(&field)?;
        let field_ty = &field.ty;

        if field_options.skip.is_some() {
            // nop
        }
        else {
            let (context_ty, context_expr) = field_options.context();
            let map_err = field_options.map_err();

            bounds.writes(field_ty, &context_ty);

            write_fields.push(quote! {
                <#field_ty as ::byst::io::Write::<__W, #context_ty>>::write(&self.#field_name, &mut __writer, #context_expr).map_err(#map_err)?;
            });
        }
    }

    Ok(quote! { #(#write_fields)* })
}

fn derive_transparent(item: DeriveInput, options: DeriveOptions) -> Result<TokenStream, Error> {
    options.check_for_transparent();

    let ident = &item.ident;

    let SplitGenerics {
        mut impl_generics,
        type_generics,
        mut where_clause,
    } = SplitGenerics::from_generics(&item.generics);
    impl_generics.type_params.push(parse_quote! { __W });
    impl_generics.type_params.push(parse_quote! { __C });
    where_clause
        .predicates
        .push(parse_quote! { __W: ::byst::io::Writer });

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::byst::io::Write<__W, __C> for #ident #type_generics #where_clause {
            type Error = <__W as ::byst::io::Writer>::Error;

            #[inline]
            fn write(&self, writer: &mut __W, _context: __C) -> Result<(), Self::Error> {
                writer.write_buf(self)
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
