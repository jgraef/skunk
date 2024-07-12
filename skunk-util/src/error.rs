use std::{
    error::Error,
    fmt::Display,
    panic::Location,
};

pub struct DisplayErrorChain<'e, E: ?Sized>(&'e E);

impl<'e, E: Error> Display for DisplayErrorChain<'e, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut e: &dyn Error = &self.0;
        write!(f, "{e}")?;
        while let Some(source) = e.source() {
            e = source;
            write!(f, "; {e}")?;
        }
        Ok(())
    }
}

pub trait ErrorExt: std::error::Error {
    #[inline]
    fn display_chain(&self) -> DisplayErrorChain<Self> {
        DisplayErrorChain(self)
    }
}

impl<E: Error> ErrorExt for E {}

pub trait ResultExt {
    fn log_error(self) -> Self;
    fn log_error_with_message(self, message: &str) -> Self;
}

impl<T, E: Error> ResultExt for Result<T, E> {
    #[inline]
    #[track_caller]
    fn log_error(self) -> Self {
        let location = Location::caller();
        if let Err(e) = &self {
            tracing::error!(
                file = make_relative(location.file()),
                line = location.line(),
                "{}",
                e.display_chain()
            );
        }
        self
    }

    #[inline]
    #[track_caller]
    fn log_error_with_message(self, message: &str) -> Self {
        let location = Location::caller();
        if let Err(e) = &self {
            tracing::error!(
                file = make_relative(location.file()),
                line = location.line(),
                "{message}: {}",
                e.display_chain()
            );
        }
        self
    }
}

fn make_relative(path: &str) -> &str {
    static DIR: Option<&'static str> = option_env!("CARGO_RUSTC_CURRENT_DIR");
    DIR.and_then(|dir| {
        path.strip_prefix(dir).map(|path| {
            path.strip_prefix(std::path::MAIN_SEPARATOR_STR)
                .unwrap_or(path)
        })
    })
    .unwrap_or(path)
}
