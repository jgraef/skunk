use darling::FromDeriveInput;
use proc_macro2::{
    Span,
    TokenStream,
};
use proc_macro_error::{
    abort,
    abort_call_site,
};
use quote::{
    quote,
    quote_spanned,
    ToTokens,
};
use syn::{
    parse_macro_input,
    punctuated::Punctuated,
    spanned::Spanned,
    token::Where,
    ConstParam,
    DeriveInput,
    Field,
    Fields,
    GenericParam,
    Generics,
    Ident,
    Index,
    Lifetime,
    LifetimeParam,
    TypeParam,
    WhereClause,
};

use crate::{
    error::Error,
    options::DeriveOptions,
};

pub fn derive_helper(
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

pub fn field_name(index: usize, field: &Field) -> (Span, TokenStream) {
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

pub fn make_where_clause(where_clause: Option<&WhereClause>) -> WhereClause {
    where_clause.cloned().unwrap_or_else(|| {
        WhereClause {
            where_token: Where {
                span: Span::call_site(),
            },
            predicates: Punctuated::new(),
        }
    })
}

pub fn exactly_one_field(fields: &Fields) -> &Field {
    let mut it = fields.iter();
    let field = it
        .next()
        .unwrap_or_else(|| abort_call_site!("Expected exactly one field."));
    if let Some(field) = it.next() {
        abort!(field.ident.span(), "Expected exactly one field.");
    }
    field
}

pub struct SplitGenerics {
    pub impl_generics: ImplGenerics,
    pub type_generics: TypeGenerics,
    pub where_clause: WhereClause,
}

impl SplitGenerics {
    pub fn from_generics(generics: &Generics) -> Self {
        Self {
            impl_generics: ImplGenerics::from_generics(generics),
            type_generics: TypeGenerics(generics.clone()),
            where_clause: make_where_clause(generics.where_clause.as_ref()),
        }
    }
}

#[derive(Default)]
pub struct ImplGenerics {
    pub lifetimes: Vec<LifetimeParam>,
    pub type_params: Vec<TypeParam>,
    pub const_params: Vec<ConstParam>,
}

impl ImplGenerics {
    pub fn from_generics(generics: &Generics) -> Self {
        let mut this = ImplGenerics::default();
        for lt in generics.lifetimes() {
            this.lifetimes.push(lt.clone());
        }
        for ty in generics.type_params() {
            this.type_params.push(ty.clone());
        }
        for con in generics.const_params() {
            this.const_params.push(con.clone());
        }
        this
    }
}

impl ToTokens for ImplGenerics {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let params = self
            .lifetimes
            .iter()
            .map(|lt| GenericParam::Lifetime(lt.clone()))
            .chain(
                self.type_params
                    .iter()
                    .map(|ty| GenericParam::Type(ty.clone())),
            )
            .chain(
                self.const_params
                    .iter()
                    .map(|con| GenericParam::Const(con.clone())),
            );

        if !self.lifetimes.is_empty()
            || !self.type_params.is_empty()
            || !self.const_params.is_empty()
        {
            tokens.extend(quote! {
                <#(#params),*>
            });
        }
    }
}

pub struct TypeGenerics(Generics);

impl ToTokens for TypeGenerics {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let params = self.0.params.iter().map(|param| {
            match param {
                GenericParam::Lifetime(lt) => TypeGenericParam::Lifetime(&lt.lifetime),
                GenericParam::Type(ty) => TypeGenericParam::Type(&ty.ident),
                GenericParam::Const(con) => TypeGenericParam::Const(&con.ident),
            }
        });

        if !self.0.params.is_empty() {
            tokens.extend(quote! {
                <#(#params),*>
            });
        }
    }
}

enum TypeGenericParam<'a> {
    Type(&'a Ident),
    Lifetime(&'a Lifetime),
    Const(&'a Ident),
}

impl<'a> ToTokens for TypeGenericParam<'a> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            TypeGenericParam::Type(ty) => ty.to_tokens(tokens),
            TypeGenericParam::Lifetime(lt) => lt.to_tokens(tokens),
            TypeGenericParam::Const(con) => con.to_tokens(tokens),
        }
    }
}
