use std::{
    cmp::Ordering,
    fmt::Debug,
};

use super::{
    chunks::WithOffset,
    BufReader,
    Length,
};
use crate::{
    impl_me,
    io::{
        End,
        Seek,
    },
    Buf,
    Range,
    RangeOutOfBounds,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Segment<B> {
    pub(crate) offset: usize,
    pub(crate) buf: B,
}

#[derive(Clone, Debug)]
pub struct Rope<B> {
    segments: Vec<Segment<B>>,
}

impl<B> Rope<B> {
    #[inline]
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            segments: Vec::with_capacity(capacity),
        }
    }

    #[inline]
    pub fn num_segments(&self) -> usize {
        self.segments.len()
    }
}

impl<B: Length> Rope<B> {
    pub fn push(&mut self, segment: B) {
        if !segment.is_empty() {
            self.segments.push(Segment {
                offset: self.len(),
                buf: segment,
            });
        }
    }

    fn view_checked(&self, range: Range) -> Result<View<'_, B>, RangeOutOfBounds> {
        let (start, end) = range.indices_checked_in(0, self.len())?;
        Ok(view_unchecked(&self.segments, start, end))
    }
}

impl<B: Buf> Buf for Rope<B> {
    type View<'a> = View<'a, B>
    where
        Self: 'a;

    type Reader<'a> = Reader<'a, B>
    where
        Self: 'a;

    #[inline]
    fn view(&self, range: impl Into<Range>) -> Result<Self::View<'_>, RangeOutOfBounds> {
        self.view_checked(range.into())
    }

    #[inline]
    fn reader(&self) -> Self::Reader<'_> {
        todo!();
    }
}

impl<B: Length> Length for Rope<B> {
    #[inline]
    fn len(&self) -> usize {
        self.segments
            .last()
            .map(|segment| segment.offset + segment.buf.len())
            .unwrap_or_default()
    }
}

impl<B: Length> FromIterator<B> for Rope<B> {
    fn from_iter<T: IntoIterator<Item = B>>(iter: T) -> Self {
        Self {
            segments: WithOffset::new(iter.into_iter())
                .map(|(offset, buf)| Segment { offset, buf })
                .collect(),
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
    segments: &'b [Segment<B>],
    start_offset: usize,
    end_offset: usize,
}

impl<'b, B: Buf> View<'b, B> {
    fn view_checked(&self, range: Range) -> Result<View<'_, B>, RangeOutOfBounds> {
        let (start, end) = range.indices_checked_in(0, self.len())?;
        let first_offset = self
            .segments
            .first()
            .map(|segment| segment.offset)
            .unwrap_or_default();
        Ok(view_unchecked(
            self.segments,
            start + first_offset + self.start_offset,
            end + first_offset + self.start_offset,
        ))
    }
}

impl<'b, B: Buf> Buf for View<'b, B> {
    type View<'a> = View<'a, B>
    where
        Self: 'a;

    type Reader<'a> = Reader<'a, B>
        where
            Self: 'a;

    #[inline]
    fn view(&self, range: impl Into<Range>) -> Result<Self::View<'_>, RangeOutOfBounds> {
        self.view_checked(range.into())
    }

    #[inline]
    fn reader(&self) -> Self::Reader<'_> {
        todo!();
    }
}

impl<'b, B: Length> Length for View<'b, B> {
    fn len(&self) -> usize {
        self.segments
            .first()
            .zip(self.segments.last())
            .map(|(first, last)| last.offset - first.offset + self.end_offset - self.start_offset)
            .unwrap_or_default()
    }
}

pub struct Reader<'a, B> {
    _segments: std::slice::Iter<'a, Segment<B>>,
}

impl<'a, B> Default for Reader<'a, B> {
    fn default() -> Self {
        Self {
            _segments: [].iter(),
        }
    }
}

impl<'a, B: Buf> BufReader for Reader<'a, B> {
    type View = View<'a, B>;

    fn peek_chunk(&self) -> Option<&[u8]> {
        todo!()
    }

    #[inline]
    fn view(&mut self, _length: usize) -> Result<Self::View, End> {
        todo!()
    }

    fn peek_view(&self, _length: usize) -> Result<Self::View, End> {
        todo!()
    }

    fn rest(&mut self) -> Self::View {
        todo!()
    }

    fn peek_rest(&self) -> Self::View {
        todo!()
    }

    fn advance(&mut self, _by: usize) -> Result<(), End> {
        todo!()
    }

    fn remaining(&self) -> usize {
        todo!()
    }
}

impl<'a, B: Buf> Seek for Reader<'a, B> {
    type Position = ();

    fn tell(&self) -> Self::Position {
        todo!();
    }

    fn seek(&mut self, _position: &Self::Position) -> Self::Position {
        todo!();
    }
}

impl_me! {
    impl['a, B: Buf] Reader for Reader<'a, B> as BufReader;
}

pub(crate) fn find_segment<S>(
    segments: &[S],
    offset: usize,
    end_spill_over: bool,
    bounds: impl Fn(&S) -> (usize, usize),
) -> Option<usize> {
    segments
        .binary_search_by(|segment| {
            let (start, end) = bounds(segment);
            match (offset.cmp(&start), offset.cmp(&end)) {
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
        .ok()
}

pub(crate) struct SegmentBounds {
    pub start_segment: usize,
    pub start_offset: usize,
    pub end_segment: usize,
    pub end_offset: usize,
}

pub(crate) fn segment_bounds_unchecked<S>(
    segments: &[S],
    start: usize,
    end: usize,
    bounds: impl Fn(&S) -> (usize, usize),
) -> Option<SegmentBounds> {
    if start == end {
        None
    }
    else {
        let start_segment =
            find_segment(segments, start, true, &bounds).expect("Bug: Didn't find start segment");
        let start_offset = start - bounds(&segments[start_segment]).0;

        let end_segment = find_segment(&segments[start_segment..], end, false, &bounds)
            .expect("Bug: Didn't find end segment")
            + start_segment;
        let end_offset = end - bounds(&segments[end_segment]).0;

        Some(SegmentBounds {
            start_segment,
            start_offset,
            end_segment,
            end_offset,
        })
    }
}

fn view_unchecked<B: Length>(segments: &[Segment<B>], start: usize, end: usize) -> View<'_, B> {
    let bounds = |segment: &Segment<B>| (segment.offset, segment.offset + segment.buf.len());

    if let Some(SegmentBounds {
        start_segment,
        start_offset,
        end_segment,
        end_offset,
    }) = segment_bounds_unchecked(segments, start, end, bounds)
    {
        View {
            segments: &segments[start_segment..=end_segment],
            start_offset,
            end_offset,
        }
    }
    else {
        View {
            segments: &[],
            start_offset: 0,
            end_offset: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buf::BufExt;

    fn collect_chunks<B: Buf>(buf: B) -> Vec<Vec<u8>> {
        // uggh... fix this when we solved the BufReader lifetime issue.
        let mut reader = buf.reader();
        let mut chunks = vec![];
        while let Some(chunk) = reader.peek_chunk() {
            chunks.push(chunk.to_owned());
            reader.advance(chunk.len()).unwrap();
        }
        chunks
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn it_chunks_correctly() {
        let input = vec![
            b"Hello" as &[u8],
            b" " as &[u8],
            b"World" as &[u8],
            b"!" as &[u8],
        ];

        let rope = input.iter().collect::<Rope<_>>();

        assert_eq!(rope.segments[0].offset, 0);
        assert_eq!(rope.segments[1].offset, 5);
        assert_eq!(rope.segments[2].offset, 6);
        assert_eq!(rope.segments[3].offset, 11);

        assert_eq!(input, collect_chunks(&rope));
    }

    #[test]
    #[ignore = "Not yet implemented"]
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
        assert_eq!(view.into_vec(), b"llo Wor");

        let view = rope.view(5..9).unwrap();
        assert_eq!(view.into_vec(), b" Wor");

        let view = rope.view(6..).unwrap();
        assert_eq!(view.into_vec(), b"World!");
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn it_chunks_corner_cases_correctly() {
        let rope = [
            b"Hello" as &[u8],
            b" " as &[u8],
            b"World" as &[u8],
            b"!" as &[u8],
        ]
        .iter()
        .collect::<Rope<_>>();

        let view = rope.view(5..6).unwrap();
        let chunks = collect_chunks(&view);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], b" ");

        let view = rope.view(5..11).unwrap();
        let chunks = collect_chunks(&view);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], b" ");
        assert_eq!(chunks[1], b"World");
    }

    #[test]
    #[ignore = "Not yet implemented"]
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
        let view2 = view.view(2..4).unwrap();
        assert_eq!(view2.into_vec(), b"o ");

        let view = rope.view(5..9).unwrap();
        let view2 = view.view(1..).unwrap();
        assert_eq!(view2.into_vec(), b"Wor");

        let view = rope.view(6..).unwrap();
        let view2 = view.view(..5).unwrap();
        assert_eq!(view2.into_vec(), b"World");
    }

    #[test]
    #[ignore = "Not yet implemented"]
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
    #[ignore = "Not yet implemented"]
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
