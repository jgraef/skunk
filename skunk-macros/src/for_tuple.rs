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
    Ident,
    Index,
    LitInt,
    Path,
    Token,
};

use crate::error::Error;

#[derive(Debug)]
pub struct ForTupleInput {
    pub callback: Path,
    pub min: usize,
    pub max: usize,
}

impl Parse for ForTupleInput {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let callback: Path = input.parse()?;

        input.parse::<Token![!]>()?;
        input.parse::<Token![for]>()?;

        let min = input
            .parse::<Option<LitInt>>()?
            .map(|n| n.base10_parse::<usize>())
            .transpose()?
            .unwrap_or_default();

        let dotdoteq = input.parse::<Option<Token![..=]>>()?;
        let to_inclusive = if dotdoteq.is_some() {
            true
        }
        else {
            if input.parse::<Option<Token![..]>>()?.is_none() {
                return Err(input.error("Expected either `..` or `..=`"));
            }
            false
        };

        let max = input.parse::<LitInt>()?.base10_parse::<usize>()?
            + to_inclusive.then_some(1).unwrap_or_default();

        Ok(Self { callback, min, max })
    }
}

pub fn for_tuple(input: ForTupleInput) -> Result<TokenStream, Error> {
    let callback = &input.callback;

    let mut indices = Vec::with_capacity(input.max);
    let mut names = Vec::with_capacity(input.max);
    let mut tys = Vec::with_capacity(input.max);
    for i in 0..input.max {
        indices.push(Index {
            index: i as u32,
            span: Span::call_site(),
        });
        names.push(Ident::new(&format!("_{}", i + 1), Span::call_site()));
        tys.push(Ident::new(&format!("T{}", i + 1), Span::call_site()));
    }
    let mut output = vec![];

    for i in input.min..input.max {
        let index = &indices[..i];
        let name = &names[..i];
        let ty = &tys[..i];

        output.push(quote! {
            #callback!(#(#index => #name: #ty),*);
        });
    }

    Ok(quote! {
        #(#output)*
    })
}
