#[cfg(feature = "axum")]
pub use crate::axum::Msgpack;

macro_rules! api_error {
    ($ident:ident $(= $status_code:ident)?) => {
        $(
            impl $crate::error::StatusCode for $ident {
                fn status_code(&self) -> ::http::StatusCode {
                    ::http::StatusCode::$status_code
                }
            }
        )?

        #[cfg(feature = "axum")]
        impl ::axum::response::IntoResponse for $ident {
            fn into_response(self) -> ::axum::response::Response {
                (
                    $crate::error::StatusCode::status_code(&self),
                    $crate::util::Msgpack(crate::error::ApiError::from(self))
                ).into_response()
            }
        }
    };
}

macro_rules! api_request {
    ($ident:ident) => {
        #[cfg(feature = "axum")]
        #[::axum::async_trait]
        impl<S> ::axum::extract::FromRequest<S> for $ident
        where
            S: Send + Sync,
        {
            type Rejection = $crate::axum::MsgpackRejection;

            async fn from_request(
                request: ::axum::extract::Request,
                state: &S,
            ) -> Result<Self, Self::Rejection> {
                Ok($crate::util::Msgpack::from_request(request, state).await?.0)
            }
        }
    };
}

macro_rules! api_response {
    ($ident:ident) => {
        #[cfg(feature = "axum")]
        impl ::axum::response::IntoResponse for $ident {
            fn into_response(self) -> ::axum::response::Response {
                $crate::util::Msgpack(self).into_response()
            }
        }
    };
}

pub(crate) use api_error;
pub(crate) use api_request;
pub(crate) use api_response;
