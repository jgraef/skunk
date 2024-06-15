use darling::{
    FromDeriveInput,
    FromField,
    FromVariant,
};
use proc_macro2::TokenStream;
use proc_macro_error::abort;
use quote::quote;
use syn::{
    self,
    parse_quote,
    Data,
    DataEnum,
    DataStruct,
    DeriveInput,
    Fields,
    WhereClause,
};

use crate::{
    error::Error,
    options::{
        Bitfield,
        BitfieldFieldOptions,
        EnumDeriveOptions,
        FieldOptions,
        StructDeriveOptions,
        VariantOptions,
    },
    util::{
        FieldName,
        SplitGenerics,
    },
};

pub fn derive_read(item: DeriveInput) -> Result<TokenStream, Error> {
    let ident = &item.ident;
    match &item.data {
        Data::Struct(s) => derive_read_for_struct(&s, &item),
        Data::Enum(e) => derive_read_for_enum(&e, &item),
        Data::Union(_) => abort!(ident, "Read can't be derive on unions."),
    }
}

fn derive_read_for_struct(s: &DataStruct, item: &DeriveInput) -> Result<TokenStream, Error> {
    let options = StructDeriveOptions::from_derive_input(&item)?;
    let ident = &item.ident;
    let (params_name, params_ty) = options.params();

    let SplitGenerics {
        mut impl_generics,
        type_generics,
        mut where_clause,
    } = SplitGenerics::from_generics(&item.generics);
    impl_generics.type_params.push(parse_quote! { __R });

    let (read_fields, struct_init) = make_struct_init(&s.fields, &mut where_clause)?;

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::byst::io::read::Read<__R, #params_ty> for #ident #type_generics #where_clause {
            fn read(mut __reader: __R, #params_name: #params_ty) -> ::std::result::Result<Self, ::byst::io::read::End> {
                #read_fields
                ::std::result::Result::Ok(Self #struct_init)
            }
        }
    })
}

fn derive_read_for_enum(e: &DataEnum, item: &DeriveInput) -> Result<TokenStream, Error> {
    let options = EnumDeriveOptions::from_derive_input(&item)?;
    let ident = &item.ident;
    let (params_name, params_ty) = options.params();

    let SplitGenerics {
        mut impl_generics,
        type_generics,
        mut where_clause,
    } = SplitGenerics::from_generics(&item.generics);
    impl_generics.type_params.push(parse_quote! { __R });

    let discriminant_expr = options.discriminant_expr(&mut where_clause);
    let mut match_arms = Vec::with_capacity(e.variants.len());

    for variant in &e.variants {
        let variant_options = VariantOptions::from_variant(&variant)?;
        let variant_name = &variant.ident;
        let pat = variant_options.pat();

        let (read_fields, struct_init) = make_struct_init(&variant.fields, &mut where_clause)?;

        match_arms.push(quote! {
            #pat => {
                #read_fields
                Self::#variant_name #struct_init
            },
        });
    }

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::byst::io::read::Read<__R, #params_ty> for #ident #type_generics #where_clause {
            fn read(mut __reader: __R, #params_name: #params_ty) -> ::std::result::Result<Self, ::byst::io::read::End> {
                let __discriminant = #discriminant_expr;
                ::std::result::Result::Ok(
                    match __discriminant {
                        #(#match_arms)*
                    }
                )
            }
        }
    })
}

fn make_struct_init(
    fields: &Fields,
    where_clause: &mut WhereClause,
) -> Result<(TokenStream, TokenStream), Error> {
    let mut read_fields = Vec::with_capacity(fields.len());
    let mut struct_init = Vec::with_capacity(fields.len());

    for (i, field) in fields.iter().enumerate() {
        let FieldName {
            span: _,
            member: field_name,
            var: field_var,
        } = FieldName::from_field(i, field);
        let field_options = FieldOptions::from_field(&field)?;
        let field_ty = &field.ty;

        if let Some(skip_with) = field_options.skip() {
            // todo: trait bound for type

            read_fields.push(quote! {
                let #field_var = #skip_with;
            });
        }
        else {
            let (params_ty, params_expr) = field_options.params();

            where_clause.predicates.push(
                parse_quote! { #field_ty: for<'__r> ::byst::io::read::Read::<&'__r mut __R, #params_ty> },
            );

            read_fields.push(quote!{
                let #field_var = <#field_ty as ::byst::io::read::Read::<_, #params_ty>>::read(&mut __reader, #params_expr)?;
            });
        }

        struct_init.push((field_name, field_var));
    }

    let struct_init = match fields {
        Fields::Named(_) => {
            let struct_init = struct_init
                .iter()
                .map(|(name, var)| quote! { #name: #var, });
            quote! { { #(#struct_init)* } }
        }
        Fields::Unnamed(_) => {
            let struct_init = struct_init.iter().map(|(_name, var)| quote! { #var, });
            quote! { ( #(#struct_init)* ) }
        }
        Fields::Unit => {
            quote! {}
        }
    };

    let read_fields = quote! { #(#read_fields)* };

    Ok((read_fields, struct_init))
}

fn derive_read_for_struct_bitfield(
    s: &DataStruct,
    bitfield: &Bitfield,
    item: &DeriveInput,
    _options: &StructDeriveOptions,
) -> Result<TokenStream, Error> {
    let ident = &item.ident;
    let SplitGenerics {
        impl_generics,
        type_generics,
        mut where_clause,
    } = SplitGenerics::from_generics(&item.generics);
    let bitfield_ty = &bitfield.ty;
    let mut struct_init = vec![];
    let mut bit_index = 0;

    let read_value = if let Some(endianness) = bitfield.endianness.ty() {
        where_clause.predicates.push(parse_quote! {
            #bitfield_ty: for<'r> ::byst::io::read::ReadXe::<&'r mut __R, #endianness>
        });
        quote! {
            ::byst::rw::ReadXe::<_, #endianness>::read(&mut reader)?
        }
    }
    else {
        where_clause.predicates.push(parse_quote! {
            #bitfield_ty: for<'r> ::byst::io::read::Read::<&'r mut __R>
        });
        quote! {
            ::byst::rw::Read::<_>::read(&mut reader)?
        }
    };

    for (i, field) in s.fields.iter().enumerate() {
        let FieldName {
            span: field_span,
            member: field_name,
            var: _,
        } = FieldName::from_field(i, field);
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
            #bitfield_ty: ::byst::BitFieldExtract<#field_ty>
        });

        struct_init.push(quote! {
            #field_name: ::byst::BitFieldExtract::extract::<#field_ty>::(#start, #bits),
        });

        bit_index = start + bits;
    }

    Ok(quote! {
        #[automatically_derived]
        impl<__R, #impl_generics> ::byst::rw::Read<__R> for #ident<#type_generics> #where_clause {
            fn read(mut reader: __R) -> ::std::result::Result<Self, ::byst::rw::End> {
                let _value: #bitfield_ty = #read_value;
                ::std::result::Result::Ok(Self {
                    #(#struct_init)*
                })
            }
        }
    })
}
