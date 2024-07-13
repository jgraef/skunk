use darling::{
    FromField,
    FromVariant,
};
use proc_macro2::TokenStream;
use proc_macro_error::{
    abort,
    abort_call_site,
};
use quote::quote;
use syn::{
    self,
    parse_quote,
    Data,
    DataEnum,
    DataStruct,
    DeriveInput,
    Fields,
};

use super::{
    Bitfield,
    BitfieldFieldOptions,
    DeriveBounds,
    DeriveOptions,
    FieldOptions,
    VariantOptions,
};
use crate::{
    error::Error,
    util::{
        Deriver,
        FieldName,
        SplitGenerics,
    },
};

pub struct DeriveRead;

impl Deriver for DeriveRead {
    const NAME: &'static str = "Read";
    type Options = DeriveOptions;

    fn derive_for_item(item: DeriveInput, options: Self::Options) -> Result<TokenStream, Error> {
        if options.transparent {
            derive_transparent(item, options)
        }
        else {
            match &item.data {
                Data::Struct(s) => Self::derive_for_struct(s, &item, options),
                Data::Enum(e) => Self::derive_for_enum(e, &item, options),
                Data::Union(u) => Self::derive_for_union(u, &item, options),
            }
        }
    }

    fn derive_for_struct(
        data: &DataStruct,
        item: &DeriveInput,
        options: Self::Options,
    ) -> Result<TokenStream, Error> {
        let ident = &item.ident;
        let (context_name, context_ty) = options.context();

        let SplitGenerics {
            mut impl_generics,
            type_generics,
            where_clause,
        } = SplitGenerics::from_generics(&item.generics);
        impl_generics.type_params.push(parse_quote! { __R });
        let mut bounds = DeriveBounds::new(where_clause, options.error.clone());

        let (read_fields, struct_init) = make_struct_init(&data.fields, &mut bounds)?;

        let (where_clause, error_ty) = bounds.finish();

        Ok(quote! {
            #[automatically_derived]
            impl #impl_generics ::byst::io::Read<__R, #context_ty> for #ident #type_generics #where_clause {
                type Error = #error_ty;

                fn read(mut __reader: &mut __R, #context_name: #context_ty) -> ::std::result::Result<Self, Self::Error> {
                    #read_fields
                    ::std::result::Result::Ok(Self #struct_init)
                }
            }
        })
    }

    fn derive_for_enum(
        data: &DataEnum,
        item: &DeriveInput,
        options: Self::Options,
    ) -> Result<TokenStream, Error> {
        let ident = &item.ident;
        let (context_name, context_ty) = options.context();

        let SplitGenerics {
            mut impl_generics,
            type_generics,
            where_clause,
        } = SplitGenerics::from_generics(&item.generics);
        impl_generics.type_params.push(parse_quote! { __R });
        let mut bounds = DeriveBounds::new(where_clause, options.error.clone());

        let tag_expr = options.tag_expr(&mut bounds);
        let mut match_arms = Vec::with_capacity(data.variants.len());

        for variant in &data.variants {
            let variant_options = VariantOptions::from_variant(variant)?;
            let variant_name = &variant.ident;
            let pat = variant_options.pat();

            let (read_fields, struct_init) = make_struct_init(&variant.fields, &mut bounds)?;

            match_arms.push(quote! {
                #pat => {
                    #read_fields
                    Self::#variant_name #struct_init
                },
            });
        }

        if !options.no_wild {
            match_arms.push(quote! {
            _ => {
                return ::std::result::Result::Err(::byst::io::InvalidDiscriminant(__tag).into());
            },
        });

            let tag_ty = options.tag_ty().unwrap_or_else(|| {
                abort_call_site!("Can't derive `Read::Error` without knowing the tag type.")
            });

            if let Some(error_ty) = &bounds.error_ty {
                bounds.where_clause.predicates.push(
                parse_quote! { #error_ty: ::std::convert::From<::byst::io::InvalidDiscriminant<#tag_ty>> },
            );
            }
            else {
                bounds.error_ty = Some(parse_quote! {
                    ::byst::io::InvalidDiscriminant<#tag_ty>
                });
            }
        }

        let (where_clause, error_ty) = bounds.finish();

        Ok(quote! {
            #[automatically_derived]
            #[allow(unreachable_code)]
            impl #impl_generics ::byst::io::Read<__R, #context_ty> for #ident #type_generics #where_clause {
                type Error = #error_ty;

                fn read(mut __reader: &mut __R, #context_name: #context_ty) -> ::std::result::Result<Self, Self::Error> {
                    let __tag = #tag_expr;
                    ::std::result::Result::Ok(
                        match __tag {
                            #(#match_arms)*
                        }
                    )
                }
            }
        })
    }
}

fn make_struct_init(
    fields: &Fields,
    bounds: &mut DeriveBounds,
) -> Result<(TokenStream, TokenStream), Error> {
    let mut read_fields = Vec::with_capacity(fields.len());
    let mut struct_init = Vec::with_capacity(fields.len());

    for (i, field) in fields.iter().enumerate() {
        let FieldName {
            span: _,
            member: field_name,
            var: field_var,
        } = FieldName::from_field(i, field);
        let field_options = FieldOptions::from_field(field)?;
        let field_ty = &field.ty;

        if let Some(skip_with) = field_options.skip() {
            // todo: trait bound for type

            read_fields.push(quote! {
                let #field_var = #skip_with;
            });
        }
        else {
            let (context_ty, context_expr) = field_options.context();
            let map_err = field_options.map_err();

            bounds.reads(field_ty, &context_ty);

            read_fields.push(quote!{
                let #field_var = <#field_ty as ::byst::io::Read::<__R, #context_ty>>::read(&mut __reader, #context_expr).map_err(#map_err)?;
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

fn derive_transparent(item: DeriveInput, options: DeriveOptions) -> Result<TokenStream, Error> {
    options.check_for_transparent();

    let ident = &item.ident;

    let SplitGenerics {
        mut impl_generics,
        type_generics,
        mut where_clause,
    } = SplitGenerics::from_generics(&item.generics);
    impl_generics.type_params.push(parse_quote! { __R });
    impl_generics.type_params.push(parse_quote! { __C });
    where_clause
        .predicates
        .push(parse_quote! { __R: ::byst::io::BufReader<View = Self> });

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::byst::io::Read<__R, __C> for #ident #type_generics #where_clause {
            type Error = ::std::convert::Infallible;

            #[inline]
            fn read(mut __reader: &mut __R, __context: __C) -> ::std::result::Result<Self, Self::Error> {
                ::std::result::Result::Ok(__reader.rest())
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
            #bitfield_ty: for<'r> ::byst::io::ReadXe::<&'r mut __R, #endianness>
        });
        quote! {
            ::byst::rw::ReadXe::<_, #endianness>::read(&mut reader)?
        }
    }
    else {
        where_clause.predicates.push(parse_quote! {
            #bitfield_ty: for<'r> ::byst::io::Read::<&'r mut __R>
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
        let field_options = BitfieldFieldOptions::from_field(field)?;
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
