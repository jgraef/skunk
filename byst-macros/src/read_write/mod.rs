mod read;
mod write;

use darling::{
    FromDeriveInput,
    FromField,
    FromMeta,
    FromVariant,
};
use proc_macro2::{
    Span,
    TokenStream,
};
use proc_macro_error::{
    abort,
    abort_call_site,
};
use quote::quote;
use syn::{
    parse::Parser,
    parse_quote,
    parse_quote_spanned,
    Expr,
    ExprLit,
    Ident,
    Lit,
    Pat,
    Path,
    Type,
    WhereClause,
};

pub use self::{
    read::DeriveRead,
    write::DeriveWrite,
};

#[derive(FromDeriveInput)]
#[darling(attributes(byst), forward_attrs(allow, doc, cfg))]
pub struct DeriveOptions {
    #[darling(default)]
    pub transparent: bool,

    pub context: Option<ContextDeriveOptions>,
    pub error: Option<Type>,

    pub tag: Option<TagDeriveOptions>,
    pub match_expr: Option<Expr>,
    #[darling(default)]
    pub no_wild: bool,
}

impl DeriveOptions {
    pub fn check_for_struct(&self) {
        if self.tag.is_some() {
            abort_call_site!("Cant use `tag` for structs.");
        }
        if self.match_expr.is_some() {
            abort_call_site!("Cant use `match_expr` for structs.");
        }
        if self.no_wild {
            abort_call_site!("Cant use `no_wild` for structs.");
        }
    }

    pub fn check_for_transparent(&self) {
        if self.context.is_some()
            || self.error.is_some()
            || self.tag.is_some()
            || self.match_expr.is_some()
            || self.no_wild
        {
            abort_call_site!("No other options are valid when deriving as view.");
        }
    }

    pub fn context(&self) -> (Ident, Type) {
        let (ident, ty) = if let Some(context) = &self.context {
            (context.name.clone(), Some(context.ty.clone()))
        }
        else {
            (None, None)
        };
        (
            ident.unwrap_or_else(|| parse_quote! { __context }),
            ty.unwrap_or_else(|| parse_quote! { () }),
        )
    }

    pub fn tag_expr(&self, track: &mut DeriveBounds) -> Expr {
        if let Some(expr) = &self.match_expr {
            expr.clone()
        }
        else if let Some(tag) = &self.tag {
            let tag_ty = &tag.ty;
            let (context_ty, context_expr) = tag.context();

            track.reads(tag_ty, &context_ty);

            parse_quote! {
                <#tag_ty as ::byst::io::Read::<_, #context_ty>>::read(&mut __reader, #context_expr)?
            }
        }
        else {
            abort_call_site!("Either a tag type, or a match expression must be specified");
        }
    }

    pub fn tag_ty(&self) -> Option<&Type> {
        self.tag.as_ref().map(|d| &d.ty)
    }
}

#[derive(FromMeta)]
pub struct ContextDeriveOptions {
    pub name: Option<Ident>,
    pub ty: Type,
}

#[derive(FromMeta)]
pub struct TagDeriveOptions {
    pub ty: Type,
    #[darling(flatten)]
    pub endianness: Endianness,
    pub context: Option<ContextFieldOptions>,
}

impl TagDeriveOptions {
    pub fn context(&self) -> (Type, Expr) {
        context(&self.endianness, self.context.as_ref())
    }
}

#[derive(FromField)]
#[darling(attributes(byst))]
pub struct FieldOptions {
    pub ident: Option<Ident>,
    pub ty: Type,

    pub skip: Option<SkipFieldOptions>,

    #[darling(flatten)]
    pub endianness: Endianness,

    pub context: Option<ContextFieldOptions>,
    pub map_err: Option<Path>,
}

impl FieldOptions {
    pub fn span(&self) -> Span {
        if let Some(ident) = &self.ident {
            ident.span()
        }
        else {
            Span::call_site()
        }
    }

    pub fn skip(&self) -> Option<Expr> {
        self.skip.as_ref().map(|skip| {
            if let Some(with) = &skip.with {
                with.clone()
            }
            else {
                let ty = &self.ty;
                parse_quote_spanned! { self.span() => <#ty as ::std::default::Default>::default() }
            }
        })
    }

    pub fn context(&self) -> (Type, Expr) {
        context(&self.endianness, self.context.as_ref())
    }

    pub fn map_err(&self) -> Path {
        self.map_err
            .clone()
            .unwrap_or_else(|| parse_quote! { std::convert::identity })
    }
}

fn context(endianness: &Endianness, context: Option<&ContextFieldOptions>) -> (Type, Expr) {
    match (endianness.ty(), context) {
        (None, None) => (parse_quote! { () }, parse_quote! { () }),
        (Some(endianness), None) => (endianness.clone(), parse_quote! { #endianness }),
        (None, Some(context)) => {
            (
                context.ty.clone(),
                context
                    .with
                    .clone()
                    .unwrap_or_else(|| parse_quote! { ::std::default::Default::default() }),
            )
        }
        _ => abort_call_site!("Endianness can not be specified, when also specifying context."),
    }
}

#[derive(FromVariant)]
#[darling(attributes(byst))]
pub struct VariantOptions {
    ident: Ident,
    discriminant: Option<Expr>,
    #[darling(rename = "tag")]
    pat: Option<DiscriminantPat>,
}

impl VariantOptions {
    pub fn span(&self) -> Span {
        self.ident.span()
    }

    pub fn pat(&self) -> Pat {
        if let Some(pat) = &self.pat {
            pat.0.clone()
        }
        else if let Some(discriminant) = &self.discriminant {
            parse_quote_spanned! { self.span() => #discriminant }
        }
        else {
            abort!(
                self.span(),
                "The variant `{}` either needs a discriminant, or a pattern specified.",
                self.ident
            )
        }
    }
}

pub struct DiscriminantPat(pub Pat);

impl FromMeta for DiscriminantPat {
    fn from_value(value: &Lit) -> Result<Self, darling::Error> {
        match value {
            Lit::Str(value) => Self::from_string(&value.value()),
            _ => {
                Ok(Self(Pat::Lit(ExprLit {
                    attrs: vec![],
                    lit: value.clone(),
                })))
            }
        }
    }

    fn from_string(value: &str) -> Result<Self, darling::Error> {
        Ok(Self(Parser::parse_str(Pat::parse_single, value)?))
    }
}

#[derive(FromMeta)]
pub struct SkipFieldOptions {
    pub with: Option<Expr>,
}

#[derive(FromMeta)]
pub struct ContextFieldOptions {
    pub ty: Type,
    pub with: Option<Expr>,
}

#[derive(FromMeta)]
pub struct Endianness {
    #[darling(default)]
    pub big: bool,

    #[darling(default)]
    pub little: bool,

    #[darling(default)]
    pub network: bool,

    #[darling(default)]
    pub native: bool,
}

impl Endianness {
    pub fn ty(&self) -> Option<Type> {
        match (self.big, self.little, self.network, self.native) {
            (false, false, false, false) => None,
            (true, false, false, false) => Some(parse_quote! { ::byst::endianness::BigEndian }),
            (false, true, false, false) => Some(parse_quote! { ::byst::endianness::LittleEndian }),
            (false, false, true, false) => Some(parse_quote! { ::byst::endianness::NetworkEndian }),
            (false, false, false, true) => Some(parse_quote! { ::byst::endianness::NativeEndian }),
            _ => {
                abort_call_site!(
                    "Only one of `big`, `little`, `network`, `native`, or `endianness = PATH` may be specified."
                )
            }
        }
    }
}

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

#[derive(FromMeta)]
pub struct Bitfield {
    pub ty: Path,

    #[darling(flatten)]
    pub endianness: Endianness,
}

#[derive(FromField)]
#[darling(attributes(byst))]
pub struct BitfieldFieldOptions {
    pub bits: Option<usize>,
    pub start: Option<usize>,
    pub end: Option<usize>,
}
