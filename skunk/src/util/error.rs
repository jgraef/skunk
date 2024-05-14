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
    fn log_error(&self);
    fn log_error_with_message(&self, message: &str);
}

impl<T, E: Error> ResultExt for Result<T, E> {
    #[inline]
    #[track_caller]
    fn log_error(&self) {
        let location = Location::caller();
        if let Err(e) = self {
            tracing::error!(
                file = make_relative(location.file()),
                line = location.line(),
                "{}",
                e.display_chain()
            );
        }
    }

    #[inline]
    #[track_caller]
    fn log_error_with_message(&self, message: &str) {
        let location = Location::caller();
        if let Err(e) = self {
            tracing::error!(
                file = make_relative(location.file()),
                line = location.line(),
                "{message}: {}",
                e.display_chain()
            );
        }
    }
}

// yeah, this could be better. probably missed a lot of edge-cases
fn make_relative(path: &str) -> &str {
    static DIR: &'static str = env!("CARGO_MANIFEST_DIR");
    DIR[..DIR.len() - 1]
        .rfind('/')
        .map(|p| &DIR[..p + 1])
        .and_then(|dir| path.strip_prefix(dir))
        .unwrap_or(path)
}
