use proc_macro2::{
    Span,
    TokenStream,
};
use quote::quote;
use syn::{
    parse::{
        Parse,
        ParseStream,
    },
    LitInt,
    Token,
};

use crate::error::Error;

pub fn bit_range(input: BitRangeInput) -> Result<TokenStream, Error> {
    let mut mask = 0u128;
    let mut bits = input.max - input.min;
    while bits > 0 {
        mask = (mask << 1) | 1;
        bits -= 1;
    }
    mask <<= input.min;

    let lit = LitInt::new(&format!("{mask}"), Span::call_site());
    Ok(quote! { #lit })
}

#[derive(Debug)]
pub struct BitRangeInput {
    pub min: usize,
    pub max: usize,
}

impl Parse for BitRangeInput {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let min = input
            .parse::<Option<LitInt>>()?
            .map(|n| n.base10_parse::<usize>())
            .transpose()?
            .unwrap_or_default();

        let dotdoteq = input.parse::<Option<Token![..=]>>()?;
        let max = if dotdoteq.is_some() {
            input.parse::<LitInt>()?.base10_parse::<usize>()? + 1
        }
        else if input.parse::<Option<Token![..]>>()?.is_some() {
            input.parse::<LitInt>()?.base10_parse::<usize>()?
        }
        else {
            min
        };

        Ok(Self { min, max })
    }
}
