use std::ops::{
    Deref,
    DerefMut,
};

use axum::{
    async_trait,
    body::Bytes,
    extract::{
        rejection::BytesRejection,
        FromRequest,
        Request,
    },
    http::{
        header,
        HeaderMap,
        HeaderValue,
        StatusCode,
    },
    response::{
        IntoResponse,
        Response,
    },
};
use mime::Mime;
use serde::{
    Deserialize,
    Serialize,
};

fn content_type(headers: &HeaderMap) -> Option<Mime> {
    headers
        .get(header::CONTENT_TYPE)?
        .to_str()
        .ok()?
        .parse::<Mime>()
        .ok()
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Msgpack<T>(pub T);

impl<T: for<'de> Deserialize<'de>> Msgpack<T> {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MsgpackRejection> {
        Ok(Self(rmp_serde::from_slice(bytes)?))
    }
}

impl<T> Deref for Msgpack<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Msgpack<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> From<T> for Msgpack<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

#[async_trait]
impl<T, S> FromRequest<S> for Msgpack<T>
where
    T: for<'de> Deserialize<'de>,
    S: Send + Sync,
{
    type Rejection = MsgpackRejection;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        if msgpack_content_type(request.headers()) {
            let bytes = Bytes::from_request(request, state).await?;
            Self::from_bytes(&bytes)
        }
        else {
            Err(MsgpackRejection::ContentType)
        }
    }
}

impl<T: Serialize> IntoResponse for Msgpack<T> {
    fn into_response(self) -> Response {
        let mut buf = Vec::with_capacity(128);
        match rmp_serde::encode::write(&mut buf, &self.0) {
            Ok(()) => {
                (
                    [(
                        header::CONTENT_TYPE,
                        HeaderValue::from_static(mime::APPLICATION_MSGPACK.as_ref()),
                    )],
                    buf,
                )
                    .into_response()
            }
            Err(e) => {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(
                        header::CONTENT_TYPE,
                        HeaderValue::from_static(mime::TEXT_PLAIN_UTF_8.as_ref()),
                    )],
                    e.to_string(),
                )
                    .into_response()
            }
        }
    }
}

fn msgpack_content_type(headers: &HeaderMap) -> bool {
    content_type(headers).map_or(false, |mime| {
        mime.type_() == "application" && mime.subtype() == "msgpack"
    })
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MsgpackRejection {
    #[error("The request is not a valid msgpack message, or the msgpack message does not match the expected format.")]
    Msgpack(#[from] rmp_serde::decode::Error),
    #[error("Expected Content-Type to be 'application/msgpack'.")]
    ContentType,
    #[error("{0}")]
    BytesRejection(#[from] BytesRejection),
}

impl IntoResponse for MsgpackRejection {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, self.to_string()).into_response()
    }
}
