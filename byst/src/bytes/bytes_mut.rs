use super::r#impl::BytesMutImpl;

pub struct BytesMut {
    _inner: Box<dyn BytesMutImpl>,
}
