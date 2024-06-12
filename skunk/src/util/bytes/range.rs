use std::{
    cmp::Ordering,
    fmt::Debug,
    ops::{
        Bound,
        RangeBounds,
    },
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub start: Bound<usize>,
    pub end: Bound<usize>,
}

impl Range {
    #[inline]
    pub fn from_range_bounds(range: impl RangeBounds<usize>) -> Self {
        Self {
            start: range.start_bound().cloned(),
            end: range.end_bound().cloned(),
        }
    }
}

macro_rules! impl_from_range_bounds {
    {
        $(
            $ty:ty;
        )*
    } => {
        $(
            impl From<$ty> for Range {
                #[inline]
                fn from(value: $ty) -> Self {
                    Self::from_range_bounds(value)
                }
            }
        )*
    };
}

impl_from_range_bounds! {
    std::ops::Range<usize>;
    std::ops::RangeFrom<usize>;
    std::ops::RangeFull;
    std::ops::RangeInclusive<usize>;
    std::ops::RangeTo<usize>;
    std::ops::RangeToInclusive<usize>;
}

impl From<usize> for Range {
    #[inline]
    fn from(value: usize) -> Self {
        Self {
            start: Bound::Included(value),
            end: Bound::Included(value),
        }
    }
}

impl<'a> From<&'a Range> for Range {
    #[inline]
    fn from(value: &'a Range) -> Self {
        *value
    }
}

impl Range {
    #[inline]
    pub fn index(&self) -> (Bound<usize>, Bound<usize>) {
        (self.start, self.end)
    }

    pub fn slice_get<'a>(&self, slice: &'a [u8]) -> Result<&'a [u8], RangeOutOfBounds> {
        slice.get(self.index()).ok_or_else(|| {
            RangeOutOfBounds {
                required: *self,
                bounds: (0, slice.len()),
            }
        })
    }

    pub fn slice_get_mut<'a>(&self, slice: &'a mut [u8]) -> Result<&'a mut [u8], RangeOutOfBounds> {
        let buf_length = slice.len();
        slice.get_mut(self.index()).ok_or_else(|| {
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
        match self.end {
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
            (Some(left), Some(right)) if left < right => false,
            _ => true,
        }
    }

    #[inline]
    pub fn contained_by(&self, other: impl Into<Range>) -> bool {
        other.into().contains(self)
    }

    pub fn contains_index(&self, index: usize) -> bool {
        match self.start() {
            Some(start) if start > index => return false,
            _ => {}
        }
        match self.end() {
            Some(end) if end < index => false,
            _ => true,
        }
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

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("Range out of bounds: {required:?} not in buffer ({}..{})", .bounds.0, .bounds.1)]
pub struct RangeOutOfBounds {
    pub required: Range,
    pub bounds: (usize, usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unbounded_range() {
        let r = Range::from(..);

        assert_eq!(r.start(), None);
        assert_eq!(r.end(), None);
        assert_eq!(r.indices_unchecked_in(12, 34), (12, 34));
        assert_eq!(r.indices_checked_in(12, 34).unwrap(), Some((12, 34)));
        assert_eq!(r.len_in(12, 34), 34 - 12);
    }

    #[test]
    fn range_with_upper_bound() {
        let r = Range::from(..4);

        assert_eq!(r.start(), None);
        assert_eq!(r.end(), Some(4));
        assert_eq!(r.indices_unchecked_in(12, 34), (12, 16));
        assert_eq!(r.indices_checked_in(12, 34).unwrap(), Some((12, 16)));
        assert_eq!(r.len_in(12, 34), 4);
    }
}
