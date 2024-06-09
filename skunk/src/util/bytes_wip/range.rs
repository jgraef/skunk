use std::{
    cmp::Ordering,
    fmt::Debug,
    ops::{
        Bound,
        RangeBounds,
    },
};

#[derive(Clone, Copy)]
pub struct Range {
    pub start: Bound<usize>,
    pub end: Bound<usize>,
}

impl<R: RangeBounds<usize>> From<R> for Range {
    fn from(value: R) -> Self {
        Self {
            start: value.start_bound().cloned(),
            end: value.end_bound().cloned(),
        }
    }
}

impl<'a> From<&'a Range> for Range {
    fn from(value: &'a Range) -> Self {
        *value
    }
}

impl Range {
    pub fn slice_get<'a>(&self, slice: &'a [u8]) -> Result<&'a [u8], RangeOutOfBounds> {
        slice.get((self.start, self.end)).ok_or_else(|| {
            RangeOutOfBounds {
                required: *self,
                bounds: (0, slice.len()),
            }
        })
    }

    pub fn slice_get_mut<'a>(&self, slice: &'a mut [u8]) -> Result<&'a mut [u8], RangeOutOfBounds> {
        let buf_length = slice.len();
        slice.get_mut((self.start, self.end)).ok_or_else(|| {
            RangeOutOfBounds {
                required: *self,
                bounds: (0, buf_length),
            }
        })
    }

    pub fn start(&self) -> Option<usize> {
        match self.start {
            Bound::Included(bound) => Some(bound),
            Bound::Excluded(bound) => Some(bound + 1),
            Bound::Unbounded => None,
        }
    }

    pub fn end(&self) -> Option<usize> {
        match self.start {
            Bound::Included(bound) => Some(bound + 1),
            Bound::Excluded(bound) => Some(bound),
            Bound::Unbounded => None,
        }
    }

    pub fn indices_unchecked_in(&self, start: usize, end: usize) -> (usize, usize) {
        let index_start = self.start().unwrap_or_default() + start;
        let index_end = self.end().map_or(end, |i| i + start);
        (index_start, index_end)
    }

    pub fn indices_checked_in(
        &self,
        start: usize,
        end: usize,
    ) -> Result<Option<(usize, usize)>, RangeOutOfBounds> {
        let (start, end) = self.indices_unchecked_in(start, end);
        match start.cmp(&end) {
            Ordering::Equal => Ok(None),
            Ordering::Less => Ok(Some((start, end))),
            Ordering::Greater => {
                Err(RangeOutOfBounds {
                    required: *self,
                    bounds: (start, end),
                })
            }
        }
    }

    #[inline]
    pub fn len_in(&self, start: usize, end: usize) -> usize {
        let (start, end) = self.indices_unchecked_in(start, end);
        end.saturating_sub(start)
    }

    pub fn contains(&self, other: impl Into<Range>) -> bool {
        let other = other.into();
        match (self.start(), other.start()) {
            (Some(left), Some(right)) if left > right => return false,
            _ => {}
        }
        match (self.end(), other.end()) {
            (Some(left), Some(right)) if left < right => return false,
            _ => {}
        }
        true
    }

    #[inline]
    pub fn contained_by(&self, other: impl Into<Range>) -> bool {
        other.into().contains(self)
    }
}

impl Debug for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.start {
            Bound::Included(start) => write!(f, "{start}")?,
            Bound::Excluded(start) => write!(f, "{}", start + 1)?,
            Bound::Unbounded => {}
        }
        write!(f, "..")?;
        match self.end {
            Bound::Included(end) => write!(f, "{end}")?,
            Bound::Excluded(end) => write!(f, "={end}")?,
            Bound::Unbounded => {}
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Range out of bounds: {required:?} not in buffer ({}..{})", .bounds.0, .bounds.1)]
pub struct RangeOutOfBounds {
    pub required: Range,
    pub bounds: (usize, usize),
}
