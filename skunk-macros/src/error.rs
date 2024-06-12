use proc_macro2::TokenStream;

#[derive(Debug, thiserror::Error)]
pub enum Error {
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
