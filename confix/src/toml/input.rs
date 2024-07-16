use std::{
    borrow::Borrow,
    fmt::Debug,
    ops::Deref,
    path::{
        Path,
        PathBuf,
    },
    sync::Arc,
};

use winnow::{
    error::Needed,
    stream::{
        AsChar,
        Compare,
        CompareResult,
        Location,
        Offset,
        Stream,
        StreamIsPartial,
    },
    PResult,
    Parser,
};

#[derive(Clone, Debug)]
pub struct Span {
    start: Pos,
    end: Pos,
    source_file: Arc<SourceFile>,
}

impl Span {
    pub fn start(&self) -> Pos {
        self.start
    }

    pub fn end(&self) -> Pos {
        self.end
    }

    pub fn source_file(&self) -> &SourceFile {
        &self.source_file
    }

    pub fn merge(&self, other: &Span) -> Span {
        assert!(Arc::ptr_eq(&self.source_file, &other.source_file));
        let start = if self.start.offset < other.start.offset {
            self.start
        }
        else {
            other.start
        };
        let end = if self.end.offset > other.end.offset {
            self.end
        }
        else {
            other.end
        };
        Span {
            start,
            end,
            source_file: self.source_file.clone(),
        }
    }
}

impl Deref for Span {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.source_file.source[self.start.offset..self.end.offset]
    }
}

impl Borrow<str> for Span {
    fn borrow(&self) -> &str {
        self.deref()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Pos {
    offset: usize,
    line: usize,
    column: usize,
}

impl Pos {
    fn offset(&self) -> usize {
        self.offset
    }

    fn line(&self) -> usize {
        self.line
    }

    fn column(&self) -> usize {
        self.column
    }
}

#[derive(Clone, Debug)]
pub struct SourceFile {
    path: Option<PathBuf>,
    source: String,
}

impl SourceFile {
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn source(&self) -> &str {
        &self.source
    }
}

#[derive(Clone, Debug)]
pub struct Input {
    source_file: Arc<SourceFile>,
    pos: Pos,
}

impl Stream for Input {
    type Token = char;
    type Slice = Span;
    type IterOffsets = CharIndices;
    type Checkpoint = Checkpoint;

    fn iter_offsets(&self) -> Self::IterOffsets {
        CharIndices {
            offset: self.pos.offset,
            source_file: self.source_file.clone(),
        }
    }

    fn eof_offset(&self) -> usize {
        self.source_file.source.len() - self.pos.offset
    }

    fn next_token(&mut self) -> Option<Self::Token> {
        let c = self.source_file.source[self.pos.offset..].chars().next()?;
        self.pos.offset += c.len();
        if c.is_newline() {
            self.pos.line += 1;
            self.pos.column = 0;
        }
        else {
            self.pos.column += 1;
        }
        Some(c)
    }

    fn offset_for<P>(&self, predicate: P) -> Option<usize>
    where
        P: Fn(Self::Token) -> bool,
    {
        for (o, c) in self.iter_offsets() {
            if predicate(c) {
                return Some(o);
            }
        }
        None
    }

    fn offset_at(&self, tokens: usize) -> Result<usize, Needed> {
        self.source_file.source[self.pos.offset..]
            .char_indices()
            .nth(tokens)
            .ok_or_else(|| Needed::Unknown)
            .map(|(offset, _)| offset)
    }

    fn next_slice(&mut self, offset: usize) -> Self::Slice {
        let start = self.pos;
        for c in self.source_file.source[self.pos.offset..][..offset].chars() {
            if c.is_newline() {
                self.pos.line += 1;
                self.pos.column = 0;
            }
            else {
                self.pos.column += 1;
            }
        }
        self.pos.offset += offset;
        Span {
            start,
            end: self.pos,
            source_file: self.source_file.clone(),
        }
    }

    fn checkpoint(&self) -> Self::Checkpoint {
        Checkpoint { pos: self.pos }
    }

    fn reset(&mut self, checkpoint: &Self::Checkpoint) {
        self.pos = checkpoint.pos;
    }

    fn raw(&self) -> &dyn Debug {
        self
    }
}

impl Offset for Input {
    fn offset_from(&self, start: &Self) -> usize {
        self.pos.offset - start.pos.offset
    }
}

impl Offset<Checkpoint> for Input {
    fn offset_from(&self, start: &Checkpoint) -> usize {
        self.pos.offset - start.pos.offset
    }
}

impl Location for Input {
    fn location(&self) -> usize {
        self.pos.offset
    }
}

impl StreamIsPartial for Input {
    type PartialState = ();

    fn complete(&mut self) -> Self::PartialState {
        // Already complete
    }

    fn restore_partial(&mut self, _state: Self::PartialState) {}

    #[inline(always)]
    fn is_partial_supported() -> bool {
        false
    }
}

impl<T> Compare<T> for Input
where
    for<'a> &'a str: Compare<T>,
{
    fn compare(&self, t: T) -> CompareResult {
        (&self.source_file.source[self.pos.offset..]).compare(t)
    }
}

#[derive(Clone, Debug)]
pub struct Checkpoint {
    pos: Pos,
}

impl Offset for Checkpoint {
    fn offset_from(&self, start: &Self) -> usize {
        self.pos.offset - start.pos.offset
    }
}

#[derive(Clone, Debug)]
pub struct CharIndices {
    offset: usize,
    source_file: Arc<SourceFile>,
}

impl Iterator for CharIndices {
    type Item = (usize, char);

    fn next(&mut self) -> Option<Self::Item> {
        let c = self.source_file.source[self.offset..]
            .char_indices()
            .next()?;
        self.offset += c.1.len();
        Some(c)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.source_file.source[self.offset..]
            .char_indices()
            .size_hint()
    }
}

pub trait ParserExt<Output, Error>: Parser<Input, Output, Error> + Sized {
    fn spanned(self) -> Spanned<Self> {
        Spanned { inner: self }
    }
}

impl<P: Parser<Input, Output, Error>, Output, Error> ParserExt<Output, Error> for P {}

pub struct Spanned<Inner> {
    inner: Inner,
}

impl<Inner, Output, Error> Parser<Input, (Span, Output), Error> for Spanned<Inner>
where
    Inner: Parser<Input, Output, Error>,
{
    fn parse_next(&mut self, input: &mut Input) -> PResult<(Span, Output), Error> {
        let start = input.pos;
        let inner = self.inner.parse_next(input)?;
        let span = Span {
            start,
            end: input.pos,
            source_file: input.source_file.clone(),
        };
        Ok((span, inner))
    }
}
