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

macro_rules! sqlx_json_type {
    ($ty:ty) => {
        #[cfg(feature = "sqlx")]
        impl<DB: sqlx::Database> sqlx::Type<DB> for $ty
        where
            sqlx::types::Json<$ty>: sqlx::Type<DB>,
        {
            fn type_info() -> <DB as sqlx::Database>::TypeInfo {
                <sqlx::types::Json<$ty> as sqlx::Type<DB>>::type_info()
            }
        }

        #[cfg(feature = "sqlx")]
        impl<'q, DB: sqlx::Database> sqlx::Encode<'q, DB> for $ty
        where
            for<'a> sqlx::types::Json<&'a $ty>: sqlx::Encode<'q, DB>,
        {
            fn encode_by_ref(
                &self,
                buf: &mut <DB as sqlx::database::HasArguments<'q>>::ArgumentBuffer,
            ) -> sqlx::encode::IsNull {
                sqlx::Encode::<'q, DB>::encode_by_ref(&sqlx::types::Json(self), buf)
            }
        }

        #[cfg(feature = "sqlx")]
        impl<'r, DB: sqlx::Database> sqlx::Decode<'r, DB> for $ty
        where
            sqlx::types::Json<$ty>: sqlx::Decode<'r, DB>,
        {
            fn decode(
                value: <DB as sqlx::database::HasValueRef<'r>>::ValueRef,
            ) -> Result<Self, sqlx::error::BoxDynError> {
                Ok(<sqlx::types::Json<$ty> as sqlx::Decode<'r, DB>>::decode(value)?.0)
            }
        }
    };
}

pub(crate) use api_error;
pub(crate) use api_request;
pub(crate) use api_response;
pub(crate) use sqlx_json_type;
