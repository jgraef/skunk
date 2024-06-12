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
};
use syn::{
    parse_macro_input,
    punctuated::Punctuated,
    spanned::Spanned,
    token::Where,
    DeriveInput,
    Field,
    Fields,
    Index,
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
