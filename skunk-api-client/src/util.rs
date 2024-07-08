use reqwest::{
    RequestBuilder,
    Response,
};
use serde::{
    Deserialize,
    Serialize,
};
use skunk_api_protocol::error::ApiError;

use crate::Error;

pub trait RequestBuilderExt: Sized {
    fn msgpack<T: Serialize>(self, body: &T) -> Result<Self, Error>;
}

impl RequestBuilderExt for RequestBuilder {
    fn msgpack<T: Serialize>(self, body: &T) -> Result<Self, Error> {
        let body = rmp_serde::to_vec(body)?;
        Ok(self.body(body))
    }
}

pub trait ResponseExt {
    async fn msgpack<T: for<'de> Deserialize<'de>>(self) -> Result<T, Error>;
}

impl ResponseExt for Response {
    async fn msgpack<T: for<'de> Deserialize<'de>>(self) -> Result<T, Error> {
        let status_code = self.status();
        let body = self.bytes().await?;

        if status_code.is_success() {
            Ok(rmp_serde::from_slice(&body)?)
        }
        else {
            // todo: currently the response can be text/plain as well
            // we should really look at the Content-Type first
            let error: ApiError = rmp_serde::from_slice(&body)?;
            Err(Error::ApiError { status_code, error })
        }
    }
}
