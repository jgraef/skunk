#![allow(dead_code)]

use super::{
    Range,
    RangeOutOfBounds,
};

/// The trait backing the [`Bytes`] implementation.
///
/// Implement this for your type, for it to be usable as a [`Bytes`]. Use
/// [`Bytes::from_impl`] to implement a conversion from your type to [`Bytes`].
pub trait BytesImpl {
    fn view(&self, range: Range) -> Result<Box<dyn BytesImpl>, RangeOutOfBounds>;
    fn chunks(
        &self,
        range: Range,
    ) -> Result<Box<dyn Iterator<Item = &[u8]> + '_>, RangeOutOfBounds>;
    fn len(&self) -> usize;
    fn clone(&self) -> Box<dyn BytesImpl>;
}
