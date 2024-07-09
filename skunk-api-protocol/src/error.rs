use std::fmt::Display;

use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    socket::SocketId,
    util::api_error,
};

pub trait StatusCode {
    fn status_code(&self) -> http::StatusCode;
}

#[derive(Debug, thiserror::Error, Serialize, Deserialize)]
#[error("API error")]
pub enum ApiError {
    Internal(#[from] InternalError),
    NoSuchSocket(#[from] NoSuchSocket),
}
api_error!(ApiError);

impl ApiError {
    pub fn internal(error: impl Display) -> Self {
        Self::Internal(InternalError {
            message: error.to_string(),
        })
    }
}

impl StatusCode for ApiError {
    fn status_code(&self) -> http::StatusCode {
        match self {
            ApiError::Internal(inner) => inner.status_code(),
            ApiError::NoSuchSocket(inner) => inner.status_code(),
        }
    }
}

#[derive(Debug, thiserror::Error, Serialize, Deserialize)]
#[error("Internal server error")]
pub struct InternalError {
    pub message: String,
}
api_error!(InternalError = INTERNAL_SERVER_ERROR);

#[derive(Debug, thiserror::Error, Serialize, Deserialize)]
#[error("No such websocket connection: {id:?}")]
pub struct NoSuchSocket {
    pub id: SocketId,
}
api_error!(NoSuchSocket = BAD_REQUEST);
