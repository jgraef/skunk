use darling::{
    FromDeriveInput,
    FromField,
    FromMeta,
    FromVariant,
};
use proc_macro2::Span;
use proc_macro_error::{
    abort,
    abort_call_site,
    emit_call_site_error,
};
use syn::{
    parse_quote,
    parse_quote_spanned,
    Expr,
    Ident,
    Lit,
    Pat,
    Path,
    Type,
};

use crate::derive_read::TrackTypes;

#[derive(FromDeriveInput)]
#[darling(attributes(byst), forward_attrs(allow, doc, cfg))]
pub struct StructDeriveOptions {
    pub bitfield: Option<Bitfield>,
    pub params: Option<ParamDeriveOptions>,
    pub error: Option<Type>,
}

impl StructDeriveOptions {
    pub fn params(&self) -> (Ident, Type) {
        let (ident, ty) = if let Some(params) = &self.params {
            (params.name.clone(), Some(params.ty.clone()))
        }
        else {
            (None, None)
        };
        (
            ident.unwrap_or_else(|| parse_quote! { __params }),
            ty.unwrap_or_else(|| parse_quote! { () }),
        )
    }
}

#[derive(FromMeta)]
pub struct ParamDeriveOptions {
    name: Option<Ident>,
    ty: Type,
}

#[derive(FromDeriveInput)]
#[darling(attributes(byst), forward_attrs(allow, doc, cfg))]
pub struct EnumDeriveOptions {
    pub discriminant: Option<DiscriminantDeriveOptions>,

    /// This can be used to implement parsing an enum without it reading the
    /// discriminant. The discriminant is supplied as parameter.
    pub params: Option<ParamDeriveOptions>,
    pub error: Option<Type>,
    pub match_expr: Option<Expr>,
    #[darling(default)]
    pub no_wild: bool,
}

impl EnumDeriveOptions {
    pub fn params(&self) -> (Ident, Type) {
        let (ident, ty) = if let Some(params) = &self.params {
            (params.name.clone(), Some(params.ty.clone()))
        }
        else {
            if self.discriminant.is_none() {
                emit_call_site_error!("You either need to specify `discriminant` or `params`. Otherwise the enum has no way to determine its discriminant.");
            }
            (None, None)
        };
        (
            ident.unwrap_or_else(|| parse_quote! { __params }),
            ty.unwrap_or_else(|| parse_quote! { () }),
        )
    }

    pub fn discriminant_expr(&self, track: &mut TrackTypes) -> Expr {
        if let Some(expr) = &self.match_expr {
            expr.clone()
        }
        else if let Some(discriminant) = &self.discriminant {
            let discriminant_ty = &discriminant.ty;
            let (params_ty, params_expr) = discriminant.params();

            track.reads(discriminant_ty, &params_ty);

            parse_quote! {
                <#discriminant_ty as ::byst::io::read::Read::<_, #params_ty>>::read(&mut __reader, #params_expr)?
            }
        }
        else {
            abort_call_site!("Either a discriminant type, or a match expression must be specified");
        }
    }

    pub fn discriminant_ty(&self) -> Option<&Type> {
        self.discriminant.as_ref().map(|d| &d.ty)
    }
}

#[derive(FromMeta)]
pub struct DiscriminantDeriveOptions {
    ty: Type,
    #[darling(flatten)]
    pub endianness: Endianness,
    pub params: Option<ParamsFieldOptions>,
}

impl DiscriminantDeriveOptions {
    pub fn params(&self) -> (Type, Expr) {
        params(&self.endianness, self.params.as_ref())
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

    pub params: Option<ParamsFieldOptions>,
    pub map_err: Option<Expr>,
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

    pub fn params(&self) -> (Type, Expr) {
        params(&self.endianness, self.params.as_ref())
    }
}

fn params(endianness: &Endianness, params: Option<&ParamsFieldOptions>) -> (Type, Expr) {
    match (endianness.ty(), params) {
        (None, None) => (parse_quote! { () }, parse_quote! { () }),
        (Some(endianness), None) => (endianness.clone(), parse_quote! { #endianness }),
        (None, Some(params)) => {
            (
                params.ty.clone(),
                params
                    .with
                    .clone()
                    .unwrap_or_else(|| parse_quote! { ::std::default::Default::default() }),
            )
        }
        _ => abort_call_site!("endianness can not be specified, when also specifying params."),
    }
}

#[derive(FromVariant)]
#[darling(attributes(byst))]
pub struct VariantOptions {
    ident: Ident,
    discriminant: Option<Expr>,
    #[darling(rename = "discriminant")]
    pat: Option<Lit>,
}

impl VariantOptions {
    pub fn span(&self) -> Span {
        self.ident.span()
    }

    pub fn pat(&self) -> Pat {
        match (&self.discriminant, &self.pat) {
            (None, None) => {
                abort!(
                    self.span(),
                    "The variant `{}` either needs a discriminant, or a pattern specified.",
                    self.ident
                )
            }
            (Some(_), Some(_)) => {
                abort!(
                    self.span(), "Only either a discriminant, or a pattern must be specified for variant `{}`, not both.", self.ident
                )
            }
            (Some(dis), None) => parse_quote! { #dis },
            (None, Some(lit)) => {
                parse_quote! { #lit }
            }
        }
    }
}

#[derive(FromMeta)]
pub struct SkipFieldOptions {
    pub with: Option<Expr>,
}

#[derive(FromMeta)]
pub struct ParamsFieldOptions {
    pub ty: Type,
    pub with: Option<Expr>,
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
