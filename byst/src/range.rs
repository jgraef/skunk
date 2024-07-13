use std::{
    fmt::Debug,
    ops::{
        Bound,
        RangeBounds,
    },
};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct Range {
    pub start: Option<usize>,
    pub end: Option<usize>,
}

impl Range {
    #[inline]
    fn from_range_bounds(range: impl RangeBounds<usize>) -> Self {
        let start = match range.start_bound() {
            Bound::Included(start) => Some(*start),
            Bound::Excluded(start) => Some(*start + 1),
            Bound::Unbounded => None,
        };
        let end = match range.end_bound() {
            Bound::Included(end) => Some(*end + 1),
            Bound::Excluded(end) => Some(*end),
            Bound::Unbounded => None,
        };

        Self { start, end }
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
            start: Some(value),
            end: Some(value + 1),
        }
    }
}

impl<'a> From<&'a Range> for Range {
    #[inline]
    fn from(value: &'a Range) -> Self {
        *value
    }
}

impl From<(usize, usize)> for Range {
    #[inline]
    fn from((start, end): (usize, usize)) -> Self {
        Self {
            start: Some(start),
            end: Some(end),
        }
    }
}

impl Range {
    pub fn with_start(mut self, start: usize) -> Self {
        self.start = Some(start);
        self
    }

    pub fn with_end(mut self, end: usize) -> Self {
        self.end = Some(end);
        self
    }

    /// # Panic
    ///
    /// Panics if `self.start.is_none()`.
    pub fn with_length(mut self, length: usize) -> Self {
        self.end = Some(self.start.expect("Range::with_length called without start") + length);
        self
    }

    #[inline]
    pub fn as_slice_index(&self) -> (Bound<usize>, Bound<usize>) {
        (
            match self.start {
                None => Bound::Unbounded,
                Some(start) => Bound::Included(start),
            },
            match self.end {
                None => Bound::Unbounded,
                Some(end) => Bound::Excluded(end),
            },
        )
    }

    pub fn slice_get<'a>(&self, slice: &'a [u8]) -> Result<&'a [u8], RangeOutOfBounds> {
        slice.get(self.as_slice_index()).ok_or({
            RangeOutOfBounds {
                required: *self,
                bounds: (0, slice.len()),
            }
        })
    }

    pub fn slice_get_mut<'a>(&self, slice: &'a mut [u8]) -> Result<&'a mut [u8], RangeOutOfBounds> {
        let buf_length = slice.len();
        slice.get_mut(self.as_slice_index()).ok_or({
            RangeOutOfBounds {
                required: *self,
                bounds: (0, buf_length),
            }
        })
    }

    pub fn indices_unchecked_in(&self, start: usize, end: usize) -> (usize, usize) {
        let index_start = self.start.unwrap_or_default() + start;
        let mut index_end = self.end.map_or(end, |i| i + start);
        if index_end < index_start {
            index_end = index_start;
        }
        (index_start, index_end)
    }

    pub fn indices_checked_in(
        &self,
        start: usize,
        end: usize,
    ) -> Result<(usize, usize), RangeOutOfBounds> {
        let err = || {
            Err(RangeOutOfBounds {
                required: *self,
                bounds: (start, end),
            })
        };

        let index_start = if let Some(range_start) = self.start {
            let index_start = range_start + start;
            if index_start > end {
                return err();
            }
            index_start
        }
        else {
            start
        };

        let index_end = if let Some(range_end) = self.end {
            let index_end = range_end + start;
            if index_end > end {
                return err();
            }

            // for now we will return RangeOutOfBounds, even though it should be
            // InvalidRange or something
            if index_end < index_start {
                return err();
            }

            index_end
        }
        else {
            end
        };

        Ok((index_start, index_end))
    }

    #[inline]
    pub fn len_in(&self, start: usize, end: usize) -> usize {
        let (start, end) = self.indices_unchecked_in(start, end);
        end.saturating_sub(start)
    }

    pub fn contains(&self, other: impl Into<Range>) -> bool {
        let other = other.into();
        match (self.start, other.start) {
            (Some(left), Some(right)) if left > right => return false,
            _ => {}
        }
        !matches!(
            (self.end, other.end),
            (Some(left), Some(right)) if left < right
        )
    }

    #[inline]
    pub fn contained_by(&self, other: impl Into<Range>) -> bool {
        other.into().contains(self)
    }

    pub fn contains_index(&self, index: usize) -> bool {
        match self.start {
            Some(start) if start > index => return false,
            _ => {}
        }
        !matches!(
            self.end,
            Some(end) if end < index
        )
    }
}

impl Debug for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(start) = self.start {
            write!(f, "{start}")?;
        }
        write!(f, "..")?;
        if let Some(end) = self.end {
            write!(f, "{end}")?;
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

        assert_eq!(r.start, None);
        assert_eq!(r.end, None);
        assert_eq!(r.indices_unchecked_in(12, 34), (12, 34));
        assert_eq!(r.indices_checked_in(12, 34).unwrap(), (12, 34));
        assert_eq!(r.len_in(12, 34), 34 - 12);
    }

    #[test]
    fn range_with_upper_bound() {
        let r = Range::from(..4);

        assert_eq!(r.start, None);
        assert_eq!(r.end, Some(4));
        assert_eq!(r.indices_unchecked_in(12, 34), (12, 16));
        assert_eq!(r.indices_checked_in(12, 34).unwrap(), (12, 16));
        assert_eq!(r.len_in(12, 34), 4);
    }
}
