use std::{
    cmp::Ordering,
    fmt::Debug,
    iter::{
        Flatten,
        Fuse,
        FusedIterator,
    },
    ops::Deref,
};

use crate::{
    util::{
        ExactSizeIter,
        IsEnd,
        IsEndIter,
        Map,
        MapFunc,
    },
    Buf,
    Range,
    RangeOutOfBounds,
};

#[derive(Clone, Copy, Debug)]
struct Segment<B> {
    offset: usize,
    buf: B,
}

#[derive(Clone, Copy, Debug)]
struct Inner<S> {
    segments: S,
}

impl<B: Buf, S: Deref<Target = [Segment<B>]>> Inner<S> {
    fn find_segment(
        &self,
        offset: usize,
        start_segment: usize,
        end_spill_over: bool,
    ) -> Option<usize> {
        let segment_index = self.segments[start_segment..]
            .binary_search_by(|segment| {
                match (
                    offset.cmp(&segment.offset),
                    offset.cmp(&(segment.offset + segment.buf.len())),
                ) {
                    // target is left of current
                    (Ordering::Less, _) => Ordering::Greater,
                    // target is definitely right of current
                    (_, Ordering::Greater) => Ordering::Less,
                    // offset falls on end of this segment. What we do here depends no
                    // `end_spill_over`. This is used to go to the next segment,
                    // if we're looking for the start of a range, or return this segment, if we're
                    // looking for the end of a range.
                    (_, Ordering::Equal) if end_spill_over => Ordering::Less,
                    // remaining cases are if this is the segment
                    _ => Ordering::Equal,
                }
            })
            .ok()?;
        Some(segment_index + start_segment)
    }

    fn view_unchecked(&self, start: usize, end: usize) -> View<'_, B> {
        if start == end {
            View {
                inner: Inner { segments: &[] },
                start_offset: 0,
                end_offset: 0,
            }
        }
        else {
            let start_segment = self
                .find_segment(start, 0, true)
                .expect("Bug: Didn't find start segment");
            let start_offset = start - self.segments[start_segment].offset;

            let end_segment = self
                .find_segment(end, start_segment, false)
                .expect("Bug: Didn't find end segment");
            let end_offset = end - self.segments[end_segment].offset;

            View {
                inner: Inner {
                    segments: &self.segments[start_segment..=end_segment],
                },
                start_offset,
                end_offset,
            }
        }
    }
}

#[derive(Debug)]
pub struct Rope<B> {
    inner: Inner<Vec<Segment<B>>>,
}

impl<B> Rope<B> {
    #[inline]
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Inner {
                segments: Vec::with_capacity(capacity),
            },
        }
    }

    #[inline]
    pub fn num_segments(&self) -> usize {
        self.inner.segments.len()
    }
}

impl<B: Buf> Rope<B> {
    pub fn push(&mut self, segment: B) {
        if !segment.is_empty() {
            self.inner.segments.push(Segment {
                offset: self.len(),
                buf: segment,
            });
        }
    }

    fn view_checked(&self, range: Range) -> Result<View<'_, B>, RangeOutOfBounds> {
        let (start, end) = range.indices_checked_in(0, self.len())?;
        Ok(self.inner.view_unchecked(start, end))
    }
}

impl<B: Buf> Buf for Rope<B> {
    type View<'a> = View<'a, B>
    where
        Self: 'a;

    type Chunks<'a> = Chunks<'a, B>
    where
        Self: 'a;

    #[inline]
    fn view(&self, range: impl Into<Range>) -> Result<Self::View<'_>, RangeOutOfBounds> {
        self.view_checked(range.into())
    }

    #[inline]
    fn chunks(&self, range: impl Into<Range>) -> Result<Self::Chunks<'_>, RangeOutOfBounds> {
        Ok(Chunks::from_view(self.view_checked(range.into())?))
    }

    #[inline]
    fn len(&self) -> usize {
        self.inner
            .segments
            .last()
            .map(|segment| segment.offset + segment.buf.len())
            .unwrap_or_default()
    }
}

impl<B: Buf> FromIterator<B> for Rope<B> {
    fn from_iter<T: IntoIterator<Item = B>>(iter: T) -> Self {
        let mut current_offset = 0;

        Self {
            inner: Inner {
                segments: iter
                    .into_iter()
                    .map(|buf| {
                        let offset = current_offset;
                        current_offset += buf.len();
                        Segment { offset, buf }
                    })
                    .collect(),
            },
        }
    }
}

impl<B> Default for Rope<B> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct View<'b, B> {
    inner: Inner<&'b [Segment<B>]>,
    start_offset: usize,
    end_offset: usize,
}

impl<'b, B: Buf> View<'b, B> {
    fn view_checked(&self, range: Range) -> Result<View<'_, B>, RangeOutOfBounds> {
        let (start, end) = range.indices_checked_in(0, self.len())?;
        let first_offset = self
            .inner
            .segments
            .first()
            .map(|segment| segment.offset)
            .unwrap_or_default();
        Ok(self.inner.view_unchecked(
            start + first_offset + self.start_offset,
            end + first_offset + self.start_offset,
        ))
    }
}

impl<'b, B: Buf> Buf for View<'b, B> {
    type View<'a> = View<'a, B>
    where
        Self: 'a;

    type Chunks<'a> = Chunks<'a, B>
    where
        Self: 'a;

    #[inline]
    fn view(&self, range: impl Into<Range>) -> Result<Self::View<'_>, RangeOutOfBounds> {
        self.view_checked(range.into())
    }

    #[inline]
    fn chunks(&self, range: impl Into<Range>) -> Result<Self::Chunks<'_>, RangeOutOfBounds> {
        Ok(Chunks::from_view(self.view_checked(range.into())?))
    }

    fn len(&self) -> usize {
        self.inner
            .segments
            .first()
            .zip(self.inner.segments.last())
            .map(|(first, last)| last.offset - first.offset + self.end_offset - self.start_offset)
            .unwrap_or_default()
    }
}

pub struct Chunks<'b, B: Buf> {
    inner: ExactSizeIter<
        Fuse<Flatten<Map<IsEndIter<std::slice::Iter<'b, Segment<B>>>, MapSegmentsToChunks>>>,
    >,
}

impl<'b, B: Buf> Chunks<'b, B> {
    #[inline]
    fn from_view(view: View<'b, B>) -> Self {
        let len = view.len();
        Self::new(view.inner.segments, view.start_offset, view.end_offset, len)
    }

    #[inline]
    fn new(segments: &'b [Segment<B>], start_offset: usize, end_offset: usize, len: usize) -> Self {
        Self {
            inner: ExactSizeIter::new(
                Map::new(
                    IsEndIter::new(segments.iter()),
                    MapSegmentsToChunks {
                        start_offset,
                        end_offset,
                    },
                )
                .flatten()
                .fuse(),
                len,
            ),
        }
    }
}

impl<'b, B: Buf> Iterator for Chunks<'b, B> {
    type Item = &'b [u8];

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<'b, B: Buf> DoubleEndedIterator for Chunks<'b, B>
where
    B::Chunks<'b>: DoubleEndedIterator,
{
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back()
    }
}

impl<'b, B: Buf> ExactSizeIterator for Chunks<'b, B> {}

impl<'b, B: Buf + Debug> Debug for Chunks<'b, B>
where
    B::Chunks<'b>: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Chunks")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<'b, B: Buf> FusedIterator for Chunks<'b, B> {}

#[derive(Debug)]
struct MapSegmentsToChunks {
    start_offset: usize,
    end_offset: usize,
}

impl<'b, B: Buf> MapFunc<IsEnd<&'b Segment<B>>> for MapSegmentsToChunks {
    type Output = B::Chunks<'b>;

    fn map(&mut self, input: IsEnd<&'b Segment<B>>) -> Self::Output {
        let mut range = Range::default();
        if input.is_start {
            range = range.with_start(self.start_offset);
        }
        if input.is_end {
            range = range.with_end(self.end_offset);
        }
        input.item.buf.chunks(range).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_chunks_correctly() {
        let input = vec![
            b"Hello" as &[u8],
            b" " as &[u8],
            b"World" as &[u8],
            b"!" as &[u8],
        ];

        let rope = input.iter().collect::<Rope<_>>();

        assert_eq!(rope.inner.segments[0].offset, 0);
        assert_eq!(rope.inner.segments[1].offset, 5);
        assert_eq!(rope.inner.segments[2].offset, 6);
        assert_eq!(rope.inner.segments[3].offset, 11);

        let chunks = rope.chunks(..).unwrap().collect::<Vec<_>>();
        assert_eq!(input, chunks);
    }

    #[test]
    fn it_views_correctly() {
        let rope = [
            b"Hello" as &[u8],
            b" " as &[u8],
            b"World" as &[u8],
            b"!" as &[u8],
        ]
        .iter()
        .collect::<Rope<_>>();

        let view = rope.view(2..9).unwrap();
        assert_eq!(view.iter(..).unwrap().collect::<Vec<_>>(), b"llo Wor");

        let view = rope.view(5..9).unwrap();
        assert_eq!(view.iter(..).unwrap().collect::<Vec<_>>(), b" Wor");

        let view = rope.view(6..).unwrap();
        assert_eq!(view.iter(..).unwrap().collect::<Vec<_>>(), b"World!");
    }

    #[test]
    fn it_chunks_corner_cases_correctly() {
        let rope = [
            b"Hello" as &[u8],
            b" " as &[u8],
            b"World" as &[u8],
            b"!" as &[u8],
        ]
        .iter()
        .collect::<Rope<_>>();

        let chunks = rope.chunks(5..6).unwrap().collect::<Vec<_>>();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], b" ");

        let chunks = rope.chunks(5..11).unwrap().collect::<Vec<_>>();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], b" ");
        assert_eq!(chunks[1], b"World");
    }

    #[test]
    fn it_views_views_correctly() {
        let rope = [
            b"Hello" as &[u8],
            b" " as &[u8],
            b"World" as &[u8],
            b"!" as &[u8],
        ]
        .iter()
        .collect::<Rope<_>>();

        let view = rope.view(2..9).unwrap();
        assert_eq!(view.iter(2..4).unwrap().collect::<Vec<_>>(), b"o ");

        let view = rope.view(5..9).unwrap();
        assert_eq!(view.iter(1..).unwrap().collect::<Vec<_>>(), b"Wor");

        let view = rope.view(6..).unwrap();
        assert_eq!(view.iter(..5).unwrap().collect::<Vec<_>>(), b"World");
    }

    #[test]
    fn len_is_correct() {
        let rope = [
            b"Hello" as &[u8],
            b" " as &[u8],
            b"World" as &[u8],
            b"!" as &[u8],
        ]
        .iter()
        .collect::<Rope<_>>();
        assert_eq!(rope.len(), 12);

        let rope = [b"" as &[u8], b"" as &[u8]].iter().collect::<Rope<_>>();
        assert_eq!(rope.len(), 0);
        assert!(rope.is_empty());
    }

    #[test]
    fn it_views_empty_ropes_correctly() {
        let rope = Rope::<&'static [u8]>::new();
        assert!(rope.view(..).unwrap().is_empty());
        assert_eq!(
            rope.view(..1).unwrap_err(),
            RangeOutOfBounds {
                required: Range::from(..1),
                bounds: (0, 0)
            }
        );
    }
}
