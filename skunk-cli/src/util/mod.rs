use axum::http::{
    header,
    HeaderMap,
};
use mime::Mime;

pub mod msgpack;
pub mod serve_ui;
pub mod watch;

pub fn content_type(headers: &HeaderMap) -> Option<Mime> {
    headers
        .get(header::CONTENT_TYPE)?
        .to_str()
        .ok()?
        .parse::<Mime>()
        .ok()
}
