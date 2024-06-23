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
    ToTokens,
};
use syn::{
    parse_macro_input,
    parse_quote,
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
    Member,
    Type,
    TypeParam,
    WhereClause,
};

use crate::error::Error;

pub fn derive_helper(
    input: proc_macro::TokenStream,
    deriver: impl FnOnce(DeriveInput) -> Result<TokenStream, Error>,
) -> proc_macro::TokenStream {
    let item = parse_macro_input!(input as DeriveInput);

    match deriver(item) {
        Ok(output) => output.into(),
        Err(e) => e.write_errors().into(),
    }
}

#[derive(Clone)]
pub struct FieldName {
    pub span: Span,
    pub member: Member,
    pub var: Ident,
}

impl FieldName {
    pub fn from_field(index: usize, field: &Field) -> Self {
        if let Some(ident) = &field.ident {
            Self {
                span: ident.span(),
                member: Member::Named(ident.clone()),
                var: ident.clone(),
            }
        }
        else {
            let span = field.ty.span();
            Self {
                span,
                member: Member::Unnamed(Index {
                    index: index as u32,
                    span,
                }),
                var: Ident::new(&format!("_{index}"), span),
            }
        }
    }
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

// better name, and move to util (this can probably be used for Write too)
pub struct DeriveBounds {
    pub where_clause: WhereClause,
    pub error_ty: Option<Type>,
}

impl DeriveBounds {
    pub fn new(where_clause: WhereClause, error_ty: Option<Type>) -> Self {
        Self {
            where_clause,
            error_ty,
        }
    }

    pub fn reads(&mut self, field_ty: &Type, context_ty: &Type) {
        self.add_bounds(field_ty, context_ty, quote! { __R }, quote! { Read })
    }

    pub fn writes(&mut self, field_ty: &Type, context_ty: &Type) {
        self.add_bounds(field_ty, context_ty, quote! { __W }, quote! { Write })
    }

    fn add_bounds(
        &mut self,
        field_ty: &Type,
        context_ty: &Type,
        io_ty: TokenStream,
        io_trait: TokenStream,
    ) {
        self.where_clause
            .predicates
            .push(parse_quote! { #field_ty: ::byst::io::#io_trait::<#io_ty, #context_ty> });

        if let Some(error_ty) = &self.error_ty {
            self.where_clause.predicates.push(
                parse_quote! { #error_ty: ::std::convert::From<<#field_ty as ::byst::io::#io_trait::<#io_ty, #context_ty>>::Error> },
            );
        }
        else {
            self.error_ty = Some(parse_quote! {
                <#field_ty as ::byst::io::#io_trait::<#io_ty, #context_ty>>::Error
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
