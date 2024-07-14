//! See [`toml.abnf`][1]
//!
//! [1]: https://github.com/toml-lang/toml/blob/1.0.0/toml.abnf

use winnow::{
    ascii::{
        line_ending,
        take_escaped,
    },
    combinator::{
        alt,
        dispatch,
        empty,
        fail,
        opt,
        peek,
        separated,
    },
    error::{
        ContextError,
        ErrMode,
        ParserError,
    },
    stream::{
        AsChar,
        Range,
        Stream,
    },
    token::{
        any,
        one_of,
        take,
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
            (Ws::parse, Table::parse, Ws::parse, opt(Comment::parse)).map(
                |(ws1, table, ws2, comment)| {
                    Self::Table {
                        ws1,
                        table,
                        ws2,
                        comment,
                    }
                },
            ),
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

fn wschar(c: char) -> bool {
    c == ' ' || c == '\t'
}

fn non_ascii(c: char) -> bool {
    matches!(
        u32::from(c),
        0x80..=0xd7ff | 0xe000..=0x10ffff
    )
}

fn non_eol(c: char) -> bool {
    non_ascii(c) || matches!(c, '\x20'..='\x7f' | '\t')
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
            .parse_next(input)
    }
}

pub enum Key<'s> {
    Simple(SimpleKey<'s>),
    Dotted(DottedKey<'s>),
}

impl<'s> Key<'s> {
    fn parse(input: &mut &'s str) -> PResult<Self> {
        alt((
            SimpleKey::parse.map(|key| Self::Simple(key)),
            DottedKey::parse.map(|key| Self::Dotted(key)),
        ))
        .parse_next(input)
    }
}

pub enum SimpleKey<'s> {
    Quoted(QuotedKey<'s>),
    Unquoted(UnquotedKey<'s>),
}

impl<'s> SimpleKey<'s> {
    fn parse(input: &mut &'s str) -> PResult<Self> {
        alt((
            QuotedKey::parse.map(|key| Self::Quoted(key)),
            UnquotedKey::parse.map(|key| Self::Unquoted(key)),
        ))
        .parse_next(input)
    }
}

pub struct UnquotedKey<'s>(pub &'s str);

impl<'s> UnquotedKey<'s> {
    fn parse(input: &mut &'s str) -> PResult<Self> {
        take_while(1.., (AsChar::is_alphanum, '-', '_'))
            .parse_next(input)
            .map(Self)
    }
}

pub enum QuotedKey<'s> {
    Basic(BasicString<'s>),
    Literal(LiteralString<'s>),
}

impl<'s> QuotedKey<'s> {
    fn parse(input: &mut &'s str) -> PResult<Self> {
        alt((
            BasicString::parse.map(|key| Self::Basic(key)),
            LiteralString::parse.map(|key| Self::Literal(key)),
        ))
        .parse_next(input)
    }
}

pub struct DottedKey<'s> {
    parts: Separated<SimpleKey<'s>, DotSep<'s>>,
}

impl<'s> DottedKey<'s> {
    fn parse(input: &mut &'s str) -> PResult<Self> {
        Separated::parser(1.., SimpleKey::parse, DotSep::parse)
            .map(|parts| Self { parts })
            .parse_next(input)
    }
}

pub struct DotSep<'s>(pub &'s str);

impl<'s> DotSep<'s> {
    fn parse(input: &mut &'s str) -> PResult<Self> {
        (Ws::parse, '.', Ws::parse)
            .recognize()
            .map(Self)
            .parse_next(input)
    }
}

pub struct KeyvalSep<'s>(pub &'s str);

impl<'s> KeyvalSep<'s> {
    fn parse(input: &mut &'s str) -> PResult<Self> {
        (Ws::parse, '=', Ws::parse)
            .recognize()
            .map(Self)
            .parse_next(input)
    }
}

/// ```plain
/// val = string / boolean / array / inline-table / date-time / float / integer
/// ```
pub enum Val<'s> {
    String(String<'s>),
    Boolean(Boolean<'s>),
    Array(Array<'s>),
    InlineTable(InlineTable<'s>),
    DateTime(DateTime<'s>),
    Float(Float<'s>),
    Integer(Integer<'s>),
}

impl<'s> Val<'s> {
    fn parse(input: &mut &'s str) -> PResult<Self> {
        alt((
            String::parse.map(|key| Self::String(key)),
            Boolean::parse.map(|key| Self::Boolean(key)),
            Array::parse.map(|key| Self::Array(key)),
            InlineTable::parse.map(|key| Self::InlineTable(key)),
            DateTime::parse.map(|key| Self::DateTime(key)),
            Float::parse.map(|key| Self::Float(key)),
            Integer::parse.map(|key| Self::Integer(key)),
        ))
        .parse_next(input)
    }
}

pub enum String<'s> {
    MlBasic(MlBasicString<'s>),
    Basic(BasicString<'s>),
    MlLiteral(MlLiteralString<'s>),
    Literal(LiteralString<'s>),
}

impl<'s> String<'s> {
    fn parse(input: &mut &'s str) -> PResult<Self> {
        alt((
            MlBasicString::parse.map(|key| Self::MlBasic(key)),
            Basic::parse.map(|key| Self::Basic(key)),
            MlLiteral::parse.map(|key| Self::MlLiteral(key)),
            Literal::parse.map(|key| Self::Literal(key)),
        ))
        .parse_next(input)
    }
}

pub struct BasicString<'s> {
    inner: Delimited<&'s str, &'s str>,
}

impl<'s> BasicString<'s> {
    /// ```plain
    /// basic-string = quotation-mark *basic-char quotation-mark
    ///
    /// quotation-mark = %x22            ; "
    ///
    /// basic-char = basic-unescaped / escaped
    /// ```
    fn parse(input: &mut &'s str) -> PResult<Self> {
        Delimited::parser(escaped(take_while(0.., basic_unescaped)), "\"")
            .map(|inner| Self { inner })
            .parse_next(input)
    }
}

/// ```plain
/// basic-unescaped = wschar / %x21 / %x23-5B / %x5D-7E / non-ascii
/// ```
fn basic_unescaped(c: char) -> bool {
    wschar(c) || c == '\x21' || matches!(c, '\x23'..='\x5b' | '\x5d'..='\x7e') | non_ascii(c)
}

/// ```plain
/// escaped = escape escape-seq-char
///
/// escape = %x5C                   ; \
/// escape-seq-char =  %x22         ; "    quotation mark  U+0022
/// escape-seq-char =/ %x5C         ; \    reverse solidus U+005C
/// escape-seq-char =/ %x62         ; b    backspace       U+0008
/// escape-seq-char =/ %x66         ; f    form feed       U+000C
/// escape-seq-char =/ %x6E         ; n    line feed       U+000A
/// escape-seq-char =/ %x72         ; r    carriage return U+000D
/// escape-seq-char =/ %x74         ; t    tab             U+0009
/// escape-seq-char =/ %x75 4HEXDIG ; uXXXX                U+XXXX
/// escape-seq-char =/ %x55 8HEXDIG ; UXXXXXXXX
/// ```
fn escaped<'s, UnescapedParser, Error>(
    unescaped: UnescapedParser,
) -> impl Parser<&'s str, &'s str, Error>
where
    UnescapedParser: Parser<&'s str, &'s str, Error>,
    Error: ParserError<&'s str>,
{
    take_escaped(
        unescaped,
        '\\',
        dispatch! {any;
            '"' => empty,
            '\\' => empty,
            'b' => empty,
            'f' => empty,
            'n' => empty,
            'r' => empty,
            't' => empty,
            'u' => take(4usize).verify(|s: &str| s.chars().all(AsChar::is_hex_digit)).void(),
            'U' => take(8usize).verify(|s: &str| s.chars().all(AsChar::is_hex_digit)).void(),
            _ => fail
        },
    )
}

pub struct MlBasicString<'s> {
    inner: Delimited<&'s str, &'s str>,
}

impl<'s> MlBasicString<'s> {
    /// ```plain
    /// ml-basic-string = ml-basic-string-delim [ newline ] ml-basic-body ml-basic-string-delim
    /// ml-basic-string-delim = 3quotation-mark
    /// ml-basic-body = *mlb-content *( mlb-quotes 1*mlb-content ) [ mlb-quotes ]
    ///
    /// mlb-content = mlb-char / newline / mlb-escaped-nl
    /// mlb-char = mlb-unescaped / escaped
    /// mlb-quotes = 1*2quotation-mark
    /// mlb-unescaped = wschar / %x21 / %x23-5B / %x5D-7E / non-ascii
    /// mlb-escaped-nl = escape ws newline *( wschar / newline )
    /// ```
    fn parse(input: &mut &'s str) -> PResult<Self> {
        todo!();
    }
}

pub struct LiteralString<'s> {
    inner: Delimited<&'s str, &'s str>,
}

impl<'s> LiteralString<'s> {
    /// ```plain
    /// literal-string = apostrophe *literal-char apostrophe
    ///
    /// apostrophe = %x27 ; ' apostrophe
    ///
    /// literal-char = %x09 / %x20-26 / %x28-7E / non-ascii
    /// ```
    fn parse(input: &mut &'s str) -> PResult<Self> {
        Delimited::parser(take_while(0.., literal_char), "'")
            .map(|inner| Self { inner })
            .parse_next(input)
    }
}

fn literal_char(c: char) -> bool {
    c == '\x09' || matches!(c, '\x20'..='\x26' | '\x28'..='\x7e') | non_ascii(c)
}

pub struct MlLiteralString<'s> {
    inner: Delimited<&'s str, &'s str>,
}

impl<'s> MlLiteralString<'s> {
    /// ```plain
    /// literal-string = apostrophe *literal-char apostrophe
    ///
    /// apostrophe = %x27 ; ' apostrophe
    ///
    /// literal-char = %x09 / %x20-26 / %x28-7E / non-ascii
    /// ```
    fn parse(input: &mut &'s str) -> PResult<Self> {
        todo!();
    }
}

pub enum Integer<'s> {
    Decimal { sign: Option<Sign>, number: &'s str },
    Hex { number: &'s str },
    Oct { number: &'s str },
    Bin { number: &'s str },
}

impl<'s> Integer<'s> {
    fn parse(input: &mut &'s str) -> PResult<Self> {
        alt((
            dec_int.map(|(sign, number)| Self::Decimal { sign, number }),
            prefixed_int,
        ))
        .parse_next(input)
    }

    fn as_int(&self) -> i64 {
        fn as_int(s: &str, base: u8) -> i64 {
            let radix = u32::from(base);
            let base = i64::from(base);
            let mut n = 0;
            for c in s.chars().rev() {
                n = n * base + i64::from(c.to_digit(radix).unwrap());
            }
            n
        }

        match self {
            Integer::Decimal { sign, number } => as_int(number, 10) * sign.map_or(1, Sign::signum),
            Integer::Hex { number } => as_int(number, 16),
            Integer::Oct { number } => as_int(number, 8),
            Integer::Bin { number } => as_int(number, 2),
        }
    }
}

fn digit(base: u8) -> fn(char) -> bool {
    match base {
        10 => |c| c.is_dec_digit(),
        16 => |c| c.is_hex_digit(),
        8 => |c| c.is_oct_digit(),
        2 => |c| c == '0' || c == '1',
        _ => panic!("Invalid base: {base}"),
    }
}

fn or_underscore(inner: impl Fn(char) -> bool) -> impl Fn(char) -> bool {
    move |c| inner(c) || c == '_'
}

fn dec_int<'s>(input: &mut &'s str) -> PResult<(Option<Sign>, &'s str)> {
    (opt(Sign::parse), unsigned_dec_int).parse_next(input)
}

fn unsigned_dec_int<'s>(input: &mut &'s str) -> PResult<&'s str> {
    let digit = digit(10);
    let digits = |input: &mut &str| -> PResult<()> {
        let first_digit = one_of(digit).parse_next(input)?;
        if first_digit != '0' {
            take_while(0.., or_underscore(digit)).parse_next(input)?;
        }
        Ok(())
    };
    digits.recognize().parse_next(input)
}

fn prefixed_int_digits<'s>(base: u8) -> impl Parser<&'s str, &'s str, ContextError> {
    let digit = digit(base);
    take_while(1.., or_underscore(digit))
}

fn prefixed_int<'s>(input: &mut &'s str) -> PResult<Integer<'s>> {
    let prefix = IntegerPrefix::parse.parse_next(input)?;
    let base = prefix.base();
    let number = prefixed_int_digits(base).parse_next(input)?;
    Ok(match base {
        16 => Integer::Hex { number },
        8 => Integer::Oct { number },
        2 => Integer::Bin { number },
        _ => panic!("Invalid base: {base}"),
    })
}

#[derive(Clone, Copy, Debug)]
pub enum Sign {
    Plus,
    Minus,
}

impl Sign {
    fn parse(input: &mut &str) -> PResult<Self> {
        dispatch! {any;
            '+' => empty.value(Self::Plus),
            '-' => empty.value(Self::Minus),
            _ => fail
        }
        .parse_next(input)
    }

    fn signum(self) -> i64 {
        match self {
            Self::Plus => 1,
            Self::Minus => -1,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum IntegerPrefix {
    Hex,
    Octal,
    Binary,
}

impl IntegerPrefix {
    fn parse(input: &mut &str) -> PResult<Self> {
        dispatch! {take(2usize);
            "0x" => empty.value(Self::Hex),
            "0o" => empty.value(Self::Octal),
            "0b" => empty.value(Self::Binary),
            _ => fail
        }
        .parse_next(input)
    }

    fn base(&self) -> u8 {
        match self {
            IntegerPrefix::Hex => 16,
            IntegerPrefix::Octal => 8,
            IntegerPrefix::Binary => 2,
        }
    }
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

pub struct Delimited<Item, Delimiter> {
    start: Delimiter,
    item: Item,
    end: Delimiter,
}

impl<Item, Delimiter> Delimited<Item, Delimiter> {
    pub fn parser<Input, ItemParser, DelimiterParser, Error>(
        item: ItemParser,
        delimiter: DelimiterParser,
    ) -> impl Parser<Input, Self, Error>
    where
        Input: Stream,
        ItemParser: Parser<Input, Item, Error>,
        DelimiterParser: Parser<Input, Delimiter, Error>,
        Error: ParserError<Input>,
    {
        todo!();
    }
}
