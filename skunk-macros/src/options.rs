use darling::{
    FromDeriveInput,
    FromField,
};
use proc_macro2::TokenStream;
use proc_macro_error::abort_call_site;
use quote::quote;
use syn::Path;

#[derive(FromDeriveInput)]
#[darling(attributes(skunk), forward_attrs(allow, doc, cfg))]
pub struct DeriveOptions {
    //ident: Ident,
    //attrs: Vec<Attribute>,
    //endianness: Option<Path>,
    pub bitfield: Option<Path>,
}

#[derive(FromField)]
#[darling(attributes(skunk))]
pub struct FieldOptions {
    pub endianness: Option<Path>,
    #[darling(default)]
    pub big: bool,
    #[darling(default)]
    pub little: bool,
    #[darling(default)]
    pub network: bool,
    #[darling(default)]
    pub native: bool,
}

impl FieldOptions {
    pub fn endianness(&self) -> Option<TokenStream> {
        match (
            self.big,
            self.little,
            self.network,
            self.native,
            &self.endianness,
        ) {
            (false, false, false, false, None) => None,
            (true, false, false, false, None) => Some(quote! { ::skunk::__private::rw::BigEndian }),
            (false, true, false, false, None) => {
                Some(quote! { ::skunk::__private::rw::LittleEndian })
            }
            (false, false, true, false, None) => {
                Some(quote! { ::skunk::__private::rw::NetworkEndian })
            }
            (false, false, false, true, None) => {
                Some(quote! { ::skunk::__private::rw::NativeEndian })
            }
            (false, false, false, false, Some(path)) => Some(quote! { #path }),
            _ => {
                abort_call_site!(
                    "Only one of `big`, `little`, `network`, `native`, or `endianness = PATH` may be specified."
                )
            }
        }
    }
}
