use super::{
    Range,
    RangeOutOfBounds,
};

pub trait BytesImpl {
    fn view(&self, range: Range) -> Result<Box<dyn BytesImpl>, RangeOutOfBounds>;
    fn chunks(
        &self,
        range: Range,
    ) -> Result<Box<dyn Iterator<Item = &[u8]> + '_>, RangeOutOfBounds>;
    fn len(&self) -> usize;
    fn clone(&self) -> Box<dyn BytesImpl>;
}
