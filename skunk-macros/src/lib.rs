use darling::FromDeriveInput;
use proc_macro2::{
    Span,
    TokenStream,
};
use proc_macro_error::{
    abort,
    proc_macro_error,
};
use quote::{
    quote,
    quote_spanned,
};
use syn::{
    parse_macro_input,
    spanned::Spanned,
    Data,
    DeriveInput,
    Field,
    Ident,
    Index,
};

#[proc_macro_error]
#[proc_macro_derive(Read)]
pub fn derive_read_macro(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    derive_helper(input, derive_read)
}

#[proc_macro_error]
#[proc_macro_derive(Write)]
pub fn derive_write_macro(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    derive_helper(input, derive_write)
}

fn derive_helper(
    input: proc_macro::TokenStream,
    deriver: impl FnOnce(DeriveInput, DeriveOptions) -> TokenStream,
) -> proc_macro::TokenStream {
    let item = parse_macro_input!(input as DeriveInput);

    let options = match DeriveOptions::from_derive_input(&item) {
        Ok(options) => options,
        Err(e) => {
            return Error::from(e).write_errors().into();
        }
    };

    deriver(item, options).into()
}

#[derive(FromDeriveInput)]
#[darling(attributes(skunk), forward_attrs(allow, doc, cfg))]
struct DeriveOptions {
    //ident: Ident,
    //attrs: Vec<Attribute>,
    endianness: Option<Ident>,
}

impl DeriveOptions {
    fn endianness_generics(&self) -> (TokenStream, TokenStream) {
        if let Some(endianness) = &self.endianness {
            (quote! {}, quote! { #endianness })
        }
        else {
            (
                quote! { __E: ::skunk::__private::Endianness, },
                quote! { __E },
            )
        }
    }
}

fn derive_read(item: DeriveInput, options: DeriveOptions) -> TokenStream {
    let ident = &item.ident;
    let (impl_generics, type_generics, where_clause) = item.generics.split_for_impl();

    let (impl_endianness, endianness) = options.endianness_generics();

    match item.data {
        Data::Struct(s) => {
            let mut read_fields = vec![];
            let mut struct_init = vec![];

            for (i, field) in s.fields.iter().enumerate() {
                let (field_span, field_name) = field_name(i, field);
                let var = Ident::new(&format!("field_{i}"), field_span);

                read_fields.push(quote! {
                    let #var = ::skunk::__private::Read::<#endianness>::read(&mut reader)?;
                });
                struct_init.push(quote! {
                    #field_name: #var,
                });
            }

            quote! {
                #[automatically_derived]
                impl<#impl_endianness #impl_generics> ::skunk::__private::Read<#endianness> for #ident<#type_generics> #where_clause {
                    fn read<R: ::skunk::__private::Reader>(mut reader: R) -> ::skunk::__private::Result<Self, ::skunk::__private::End> {
                        #(#read_fields)*
                        Ok(Self {
                            #(#struct_init)*
                        })
                    }
                }
            }
        }
        Data::Enum(_) => todo!(),
        Data::Union(_) => abort!(ident, "Can't derive Read for union"),
    }
}

fn derive_write(item: DeriveInput, options: DeriveOptions) -> TokenStream {
    let ident = &item.ident;
    let (impl_generics, type_generics, where_clause) = item.generics.split_for_impl();

    let (impl_endianness, endianness) = options.endianness_generics();

    match item.data {
        Data::Struct(s) => {
            let mut write_fields = vec![];

            for (i, field) in s.fields.iter().enumerate() {
                let (_, field_name) = field_name(i, field);

                write_fields.push(quote! {
                    ::skunk::__private::Write::<#endianness>::write(&self.#field_name, &mut writer)?;
                });
            }

            quote! {
                #[automatically_derived]
                impl<#impl_endianness #impl_generics> ::skunk::__private::Write<#endianness> for #ident<#type_generics> #where_clause {
                    fn write<W: ::skunk::__private::Writer>(&self, mut writer: W) -> ::skunk::__private::Result<(), ::skunk::__private::Full> {
                        #(#write_fields)*
                        ::skunk::__private::Ok(())
                    }
                }
            }
        }
        Data::Enum(_) => todo!(),
        Data::Union(_) => abort!(ident, "Can't derive Read for union"),
    }
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("syn error")]
    Syn(#[from] syn::Error),

    #[error("darling error")]
    Darling(#[from] darling::Error),
}

impl Error {
    pub fn write_errors(self) -> TokenStream {
        match self {
            Error::Syn(e) => e.into_compile_error(),
            Error::Darling(e) => e.write_errors(),
        }
    }
}

fn field_name(index: usize, field: &Field) -> (Span, TokenStream) {
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
