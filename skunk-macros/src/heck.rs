use proc_macro2::TokenStream;
use proc_macro_error::abort_call_site;
use quote::ToTokens;
use syn::{
    parse::{
        Parse,
        ParseStream,
    },
    Ident,
    LitStr,
};

pub enum IdentOrLitStr {
    Ident(Ident),
    LitStr(LitStr),
}

impl Parse for IdentOrLitStr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if let Ok(ident) = input.parse::<Ident>() {
            Ok(Self::Ident(ident))
        }
        else if let Ok(lit_str) = input.parse::<LitStr>() {
            Ok(Self::LitStr(lit_str))
        }
        else {
            abort_call_site!("Expected either an identifier, or string literal");
        }
    }
}

impl IdentOrLitStr {
    pub fn convert_case(self, f: impl FnOnce(&str) -> String) -> Self {
        match self {
            IdentOrLitStr::Ident(ident) => {
                Self::Ident(Ident::new(&f(&ident.to_string()), ident.span()))
            }
            IdentOrLitStr::LitStr(lit_str) => {
                Self::LitStr(LitStr::new(&f(&lit_str.value()), lit_str.span()))
            }
        }
    }
}

impl ToTokens for IdentOrLitStr {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            IdentOrLitStr::Ident(ident) => ident.to_tokens(tokens),
            IdentOrLitStr::LitStr(lit_str) => lit_str.to_tokens(tokens),
        }
    }
}
