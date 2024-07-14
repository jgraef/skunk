//! See [`toml.abnf`][1]
//!
//! [1]: https://github.com/toml-lang/toml/blob/1.0.0/toml.abnf

use winnow::{
    ascii::line_ending,
    combinator::{
        alt,
        opt,
        separated,
    },
    error::{
        ContextError,
        ErrMode,
        ParserError,
    },
    stream::{
        Range,
        Stream,
    },
    token::{
        one_of,
        take_until,
        take_while,
    },
    PResult,
    Parser,
};

pub struct Toml<'s> {
    pub expressions: Separated<Expression<'s>, Newline<'s>>,
}

impl<'s> Toml<'s> {
    /// ```plain
    /// toml = expression *( newline expression )
    /// ```
    fn parse(input: &mut &'s str) -> PResult<Self> {
        Separated::parser(1.., Expression::parse, Newline::parse)
            .map(|expressions| Self { expressions })
            .parse_next(input)
    }
}

pub enum Expression<'s> {
    Comment {
        ws: Ws<'s>,
        comment: Option<Comment<'s>>,
    },
    Keyval {
        ws1: Ws<'s>,
        keyval: Keyval<'s>,
        ws2: Ws<'s>,
        comment: Option<Comment<'s>>,
    },
    Table {
        ws1: Ws<'s>,
        table: Table<'s>,
        ws2: Ws<'s>,
        comment: Option<Comment<'s>>,
    },
}

impl<'s> Expression<'s> {
    /// ```plain
    /// expression =  ws [ comment ]
    /// expression =/ ws keyval ws [ comment ]
    /// expression =/ ws table ws [ comment ]
    /// ```
    fn parse(input: &mut &'s str) -> PResult<Self> {
        alt((
            (Ws::parse, opt(Comment::parse)).map(|(ws, comment)| Self::Comment { ws, comment }),
            (Ws::parse, Keyval::parse, Ws::parse, opt(Comment::parse)).map(
                |(ws1, keyval, ws2, comment)| {
                    Self::Keyval {
                        ws1,
                        keyval,
                        ws2,
                        comment,
                    }
                },
            ),
            (Ws::parse, table, Ws::parse, opt(Comment::parse)).map(|(ws1, table, ws2, comment)| {
                Self::Table {
                    ws1,
                    table,
                    ws2,
                    comment,
                }
            }),
        ))
        .parse_next(input)
    }
}

pub struct Ws<'s>(pub &'s str);

impl<'s> Ws<'s> {
    /// ```plain
    /// ws = *wschar
    /// wschar =  %x20  ; Space
    /// wschar =/ %x09  ; Horizontal tab
    /// ```
    fn parse(input: &mut &'s str) -> PResult<Self> {
        take_while(0.., |c| c == ' ' || c == '\t')
            .recognize()
            .map(Self)
            .parse_next(input)
    }
}

pub struct Newline<'s>(pub &'s str);

impl<'s> Newline<'s> {
    fn parse(input: &mut &'s str) -> PResult<Self> {
        line_ending.parse_next(input).map(Self)
    }
}

pub struct Comment<'s>(pub &'s str);

impl<'s> Comment<'s> {
    /// ```plain
    /// comment-start-symbol = %x23 ; #
    /// non-ascii = %x80-D7FF / %xE000-10FFFF
    /// non-eol = %x09 / %x20-7F / non-ascii
    ///
    /// comment = comment-start-symbol *non-eol
    /// ```
    fn parse(input: &mut &'s str) -> PResult<Self> {
        fn non_eol(c: char) -> bool {
            matches!(
                u32::from(c),
                0x80..=0xd7ff | 0xe000..=0x10ffff | 0x09 | 0x20..=0x70
            )
        }

        ("#", take_while(0.., non_eol))
            .recognize()
            .map(Self)
            .parse_next(input)
    }
}

pub struct Keyval<'s> {
    pub key: Key<'s>,
    pub sep: KeyvalSep<'s>,
    pub val: Val<'s>,
}

impl<'s> Keyval<'s> {
    fn parse(input: &mut &'s str) -> PResult<Self> {
        (Key::parse, KeyvalSep::parse, Val::parse)
            .map(|(key, sep, val)| Self { key, sep, val })
            .parse_next()
    }
}

pub enum Key<'s> {
    Simple(SimpleKey<'s>),
    Dotted(DottedKey<'s>),
}

pub enum SimpleKey<'s> {
    Quoted(QuotedKey<'s>),
    Unquoted(UnquotedKey<'s>),
}

pub struct UnquotedKey<'s>(pub &'s str);

pub enum QuotedKey<'s> {
    Basic(BasicString<'s>),
    Literal(LiteralString<'s>),
}

pub struct DottedKey<'s> {
    parts: Separated<SimpleKey<'s>, DotSep<'s>>,
}

pub struct Table<'s> {}

pub struct WithInput<'s, T> {
    pub input: &'s str,
    pub inner: T,
}

pub struct Separated<Item, Sep> {
    items: Vec<Item>,
    seps: Vec<Sep>,
}

impl<Item, Sep> Separated<Item, Sep> {
    pub fn parser<Input, ItemParser, SepParser, Error>(
        occurences: impl Into<Range>,
        item: ItemParser,
        separator: SepParser,
    ) -> impl Parser<Input, Self, Error>
    where
        Input: Stream,
        ItemParser: Parser<Input, Item, Error>,
        SepParser: Parser<Input, Sep, Error>,
        Error: ParserError<Input>,
    {
        todo!();
    }
}
