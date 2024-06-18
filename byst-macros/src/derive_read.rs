use darling::{
    FromDeriveInput,
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
    Type,
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
        where_clause,
    } = SplitGenerics::from_generics(&item.generics);
    impl_generics.type_params.push(parse_quote! { __R });
    let mut track = TrackTypes::new(where_clause, options.error.clone());

    let (read_fields, struct_init) = make_struct_init(&s.fields, &mut track)?;

    let (where_clause, error_ty) = track.finish();

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::byst::io::read::Read<__R, #params_ty> for #ident #type_generics #where_clause {
            type Error = #error_ty;

            fn read(mut __reader: &mut __R, #params_name: #params_ty) -> ::std::result::Result<Self, Self::Error> {
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
        where_clause,
    } = SplitGenerics::from_generics(&item.generics);
    impl_generics.type_params.push(parse_quote! { __R });
    let mut track = TrackTypes::new(where_clause, options.error.clone());

    let discriminant_expr = options.discriminant_expr(&mut track);
    let mut match_arms = Vec::with_capacity(e.variants.len());

    for variant in &e.variants {
        let variant_options = VariantOptions::from_variant(&variant)?;
        let variant_name = &variant.ident;
        let pat = variant_options.pat();

        let (read_fields, struct_init) = make_struct_init(&variant.fields, &mut track)?;

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
                return ::std::result::Result::Err(::byst::io::read::InvalidDiscriminant(__discriminant).into());
            },
        });

        let discriminant_ty = options.discriminant_ty().unwrap_or_else(|| {
            abort_call_site!("Can't derive `Read::Error` without knowing the discriminant type.")
        });

        if let Some(error_ty) = &track.error_ty {
            track.where_clause.predicates.push(
                parse_quote! { #error_ty: ::std::convert::From<::byst::io::read::InvalidDiscriminant<#discriminant_ty>> },
            );
        }
        else {
            track.error_ty = Some(parse_quote! {
                ::byst::io::read::InvalidDiscriminant<#discriminant_ty>
            });
        }
    }

    let (where_clause, error_ty) = track.finish();

    Ok(quote! {
        #[automatically_derived]
        #[allow(unreachable_code)]
        impl #impl_generics ::byst::io::read::Read<__R, #params_ty> for #ident #type_generics #where_clause {
            type Error = #error_ty;

            fn read(mut __reader: &mut __R, #params_name: #params_ty) -> ::std::result::Result<Self, Self::Error> {
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
    track: &mut TrackTypes,
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

            track.reads(field_ty, &params_ty);

            read_fields.push(quote!{
                let #field_var = <#field_ty as ::byst::io::read::Read::<__R, #params_ty>>::read(&mut __reader, #params_expr)?;
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

// better name, and move to util (this can probably be used for Write too)
pub struct TrackTypes {
    where_clause: WhereClause,
    error_ty: Option<Type>,
}

impl TrackTypes {
    pub fn new(where_clause: WhereClause, error_ty: Option<Type>) -> Self {
        Self {
            where_clause,
            error_ty,
        }
    }

    pub fn reads(&mut self, field_ty: &Type, params_ty: &Type) {
        self.where_clause
            .predicates
            .push(parse_quote! { #field_ty: ::byst::io::read::Read::<__R, #params_ty> });

        if let Some(error_ty) = &self.error_ty {
            self.where_clause.predicates.push(
                parse_quote! { #error_ty: ::std::convert::From<<#field_ty as ::byst::io::read::Read::<__R, #params_ty>>::Error> },
            );
        }
        else {
            self.error_ty = Some(parse_quote! {
                <#field_ty as ::byst::io::read::Read<__R, #params_ty>>::Error
            });
        }
    }

    pub fn finish(self) -> (WhereClause, Type) {
        let error_ty = self
            .error_ty
            .unwrap_or_else(|| parse_quote! { ::std::convert::Infallible });
        (self.where_clause, error_ty)
    }
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