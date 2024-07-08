use reqwest::StatusCode;
use skunk_api_protocol::error::ApiError;

#[derive(Debug, thiserror::Error)]
#[error("API client error")]
pub enum Error {
    Reqwest(#[from] reqwest::Error),
    Websocket(#[from] reqwest_websocket::Error),
    Decode(#[from] rmp_serde::decode::Error),
    Encode(#[from] rmp_serde::encode::Error),
    ApiError {
        status_code: StatusCode,
        #[source]
        error: ApiError,
    },
}
