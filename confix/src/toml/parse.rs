//! See [`toml.abnf`][1]
//!
//! [1]: https://github.com/toml-lang/toml/blob/1.0.0/toml.abnf

use std::ops::{
    Bound,
    RangeBounds,
};

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
        trace,
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
        take_while,
    },
    PResult,
    Parser,
};

use super::input::{
    Input,
    ParserExt,
    Span,
};

pub struct Toml {
    pub span: Span,
    pub expressions: Separated<Expression, Newline>,
}

impl Toml {
    /// ```plain
    /// toml = expression *( newline expression )
    /// ```
    fn parse(input: &mut Input) -> PResult<Self> {
        Separated::parser(1.., Expression::parse, Newline::parse)
            .spanned()
            .map(|(span, expressions)| Self { span, expressions })
            .parse_next(input)
    }
}

pub enum Expression {
    Comment {
        span: Span,
        ws: Whitespace,
        comment: Option<Comment>,
    },
    Keyval {
        span: Span,
        ws1: Whitespace,
        keyval: KeyValue,
        ws2: Whitespace,
        comment: Option<Comment>,
    },
    Table {
        span: Span,
        ws1: Whitespace,
        table: Table,
        ws2: Whitespace,
        comment: Option<Comment>,
    },
}

impl Expression {
    /// ```plain
    /// expression =  ws [ comment ]
    /// expression =/ ws keyval ws [ comment ]
    /// expression =/ ws table ws [ comment ]
    /// ```
    fn parse(input: &mut Input) -> PResult<Self> {
        alt((
            (Whitespace::parse, opt(Comment::parse))
                .spanned()
                .map(|(span, (ws, comment))| Self::Comment { span, ws, comment }),
            (
                Whitespace::parse,
                KeyValue::parse,
                Whitespace::parse,
                opt(Comment::parse),
            )
                .spanned()
                .map(|(span, (ws1, keyval, ws2, comment))| {
                    Self::Keyval {
                        span,
                        ws1,
                        keyval,
                        ws2,
                        comment,
                    }
                }),
            (
                Whitespace::parse,
                Table::parse,
                Whitespace::parse,
                opt(Comment::parse),
            )
                .spanned()
                .map(|(span, (ws1, table, ws2, comment))| {
                    Self::Table {
                        span,
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

pub struct Whitespace {
    span: Span,
}

impl Whitespace {
    /// ```plain
    /// ws = *wschar
    /// wschar =  %x20  ; Space
    /// wschar =/ %x09  ; Horizontal tab
    /// ```
    fn parse(input: &mut Input) -> PResult<Self> {
        take_while(0.., wschar)
            .recognize()
            .map(|span| Self { span })
            .parse_next(input)
    }
}

pub struct Newline {
    span: Span,
}

impl Newline {
    fn parse(input: &mut Input) -> PResult<Self> {
        line_ending
            .recognize()
            .map(|span| Self { span })
            .parse_next(input)
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

pub struct Comment {
    span: Span,
}

impl Comment {
    /// ```plain
    /// comment-start-symbol = %x23 ; #
    /// non-ascii = %x80-D7FF / %xE000-10FFFF
    /// non-eol = %x09 / %x20-7F / non-ascii
    ///
    /// comment = comment-start-symbol *non-eol
    /// ```
    fn parse(input: &mut Input) -> PResult<Self> {
        ("#", take_while(0.., non_eol))
            .recognize()
            .map(|span| Self { span })
            .parse_next(input)
    }
}

pub struct KeyValue {
    span: Span,
    key: Key,
    sep: KeyvalSep,
    val: Value,
}

impl KeyValue {
    fn parse(input: &mut Input) -> PResult<Self> {
        (Key::parse, KeyvalSep::parse, Value::parse)
            .spanned()
            .map(|(span, (key, sep, val))| {
                Self {
                    span,
                    key,
                    sep,
                    val,
                }
            })
            .parse_next(input)
    }
}

pub enum Key {
    Simple(SimpleKey),
    Dotted(DottedKey),
}

impl Key {
    fn parse(input: &mut Input) -> PResult<Self> {
        alt((
            SimpleKey::parse.map(|key| Self::Simple(key)),
            DottedKey::parse.map(|key| Self::Dotted(key)),
        ))
        .parse_next(input)
    }
}

pub enum SimpleKey {
    Quoted(QuotedKey),
    Unquoted(UnquotedKey),
}

impl SimpleKey {
    fn parse(input: &mut Input) -> PResult<Self> {
        alt((
            QuotedKey::parse.map(|key| Self::Quoted(key)),
            UnquotedKey::parse.map(|key| Self::Unquoted(key)),
        ))
        .parse_next(input)
    }
}

pub struct UnquotedKey {
    span: Span,
}

impl UnquotedKey {
    fn parse(input: &mut Input) -> PResult<Self> {
        take_while(1.., (AsChar::is_alphanum, '-', '_'))
            .parse_next(input)
            .map(|span| Self { span })
    }
}

pub enum QuotedKey {
    Basic(BasicString),
    Literal(LiteralString),
}

impl QuotedKey {
    fn parse(input: &mut Input) -> PResult<Self> {
        alt((
            BasicString::parse.map(|key| Self::Basic(key)),
            LiteralString::parse.map(|key| Self::Literal(key)),
        ))
        .parse_next(input)
    }
}

pub struct DottedKey {
    span: Span,
    parts: Separated<SimpleKey, DotSep>,
}

impl DottedKey {
    fn parse(input: &mut Input) -> PResult<Self> {
        Separated::parser(1.., SimpleKey::parse, DotSep::parse)
            .spanned()
            .map(|(span, parts)| Self { span, parts })
            .parse_next(input)
    }
}

pub struct DotSep {
    span: Span,
}

impl DotSep {
    fn parse(input: &mut Input) -> PResult<Self> {
        (Whitespace::parse, '.', Whitespace::parse)
            .recognize()
            .map(|span| Self { span })
            .parse_next(input)
    }
}

pub struct KeyvalSep {
    span: Span,
}

impl KeyvalSep {
    fn parse(input: &mut Input) -> PResult<Self> {
        (Whitespace::parse, '=', Whitespace::parse)
            .recognize()
            .map(|span| Self { span })
            .parse_next(input)
    }
}

/// ```plain
/// val = string / boolean / array / inline-table / date-time / float / integer
/// ```
pub enum Value {
    String(String),
    Boolean(Boolean),
    Array(Array),
    InlineTable(InlineTable),
    DateTime(DateTime),
    Float(Float),
    Integer(Integer),
}

impl Value {
    fn parse(input: &mut Input) -> PResult<Self> {
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

pub enum String {
    MlBasic(MlBasicString),
    Basic(BasicString),
    MlLiteral(MlLiteralString),
    Literal(LiteralString),
}

impl String {
    fn parse(input: &mut Input) -> PResult<Self> {
        alt((
            MlBasicString::parse.map(|key| Self::MlBasic(key)),
            BasicString::parse.map(|key| Self::Basic(key)),
            MlLiteralString::parse.map(|key| Self::MlLiteral(key)),
            LiteralString::parse.map(|key| Self::Literal(key)),
        ))
        .parse_next(input)
    }
}

pub struct BasicString {
    span: Span,
    inner: Delimited<Span, Span>,
}

impl BasicString {
    /// ```plain
    /// basic-string = quotation-mark *basic-char quotation-mark
    ///
    /// quotation-mark = %x22            ; "
    ///
    /// basic-char = basic-unescaped / escaped
    /// ```
    fn parse(input: &mut Input) -> PResult<Self> {
        Delimited::parser(escaped(take_while(0.., basic_unescaped)), "\"")
            .spanned()
            .map(|(span, inner)| Self { span, inner })
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
) -> impl Parser<Input, Span, Error>
where
    UnescapedParser: Parser<Input, Span, Error>,
    Error: ParserError<Input>,
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

pub struct MlBasicString {
    span: Span,
    inner: Delimited<Span, Span>,
}

impl MlBasicString {
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
    fn parse(input: &mut Input) -> PResult<Self> {
        todo!();
    }
}

pub struct LiteralString {
    span: Span,
    inner: Delimited<Span, Span>,
}

impl LiteralString {
    /// ```plain
    /// literal-string = apostrophe *literal-char apostrophe
    ///
    /// apostrophe = %x27 ; ' apostrophe
    ///
    /// literal-char = %x09 / %x20-26 / %x28-7E / non-ascii
    /// ```
    fn parse(input: &mut Input) -> PResult<Self> {
        Delimited::parser(take_while(0.., literal_char), "'")
            .spanned()
            .map(|(span, inner)| Self { span, inner })
            .parse_next(input)
    }
}

fn literal_char(c: char) -> bool {
    c == '\x09' || matches!(c, '\x20'..='\x26' | '\x28'..='\x7e') | non_ascii(c)
}

pub struct MlLiteralString {
    span: Span,
    inner: Delimited<Span, Span>,
}

impl MlLiteralString {
    /// ```plain
    /// literal-string = apostrophe *literal-char apostrophe
    ///
    /// apostrophe = %x27 ; ' apostrophe
    ///
    /// literal-char = %x09 / %x20-26 / %x28-7E / non-ascii
    /// ```
    fn parse(input: &mut Input) -> PResult<Self> {
        todo!();
    }
}

pub enum Integer {
    Dec {
        span: Span,
        sign: Option<Sign>,
        digits: Digits,
    },
    Hex {
        span: Span,
        prefix: IntegerPrefix,
        digits: Digits,
    },
    Oct {
        span: Span,
        prefix: IntegerPrefix,
        digits: Digits,
    },
    Bin {
        span: Span,
        prefix: IntegerPrefix,
        digits: Digits,
    },
}

impl Integer {
    fn parse(input: &mut Input) -> PResult<Self> {
        alt((
            dec_int
                .spanned()
                .map(|(span, (sign, digits))| Self::Dec { span, sign, digits }),
            prefixed_int,
        ))
        .parse_next(input)
    }

    fn as_int(&self) -> i64 {
        match self {
            Integer::Dec { sign, digits, .. } => {
                digits.as_int(10) * sign.as_ref().map_or(1, Sign::signum)
            }
            Integer::Hex { digits, .. } => digits.as_int(16),
            Integer::Oct { digits, .. } => digits.as_int(8),
            Integer::Bin { digits, .. } => digits.as_int(2),
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

fn dec_int(input: &mut Input) -> PResult<(Option<Sign>, Digits)> {
    (opt(Sign::parse), Digits::parse_dec).parse_next(input)
}

fn prefixed_int(input: &mut Input) -> PResult<Integer> {
    fn inner(input: &mut Input) -> PResult<(IntegerPrefix, Digits)> {
        let prefix = IntegerPrefix::parse.parse_next(input)?;
        let digits = Digits::with_leading_zeros(prefix.base()).parse_next(input)?;
        Ok((prefix, digits))
    }

    inner
        .spanned()
        .map(|(span, (prefix, digits))| {
            match &prefix {
                IntegerPrefix::Hex { .. } => {
                    Integer::Hex {
                        span,
                        prefix,
                        digits,
                    }
                }
                IntegerPrefix::Oct { .. } => {
                    Integer::Oct {
                        span,
                        prefix,
                        digits,
                    }
                }
                IntegerPrefix::Bin { .. } => {
                    Integer::Bin {
                        span,
                        prefix,
                        digits,
                    }
                }
            }
        })
        .parse_next(input)
}

pub struct Digits {
    span: Span,
}

impl Digits {
    fn parse_dec(input: &mut Input) -> PResult<Digits> {
        let digit = digit(10);
        let digits = |input: &mut Input| -> PResult<()> {
            let first_digit = one_of(digit).parse_next(input)?;
            if first_digit != '0' {
                take_while(0.., or_underscore(digit)).parse_next(input)?;
            }
            Ok(())
        };
        digits
            .recognize()
            .map(|span| Digits { span })
            .parse_next(input)
    }

    fn with_leading_zeros(base: u8) -> impl Parser<Input, Digits, ContextError> {
        let digit = digit(base);
        take_while(1.., or_underscore(digit)).map(|span| Digits { span })
    }

    fn as_int(&self, base: u8) -> i64 {
        let radix = u32::from(base);
        let base = i64::from(base);

        let mut n = 0;
        for c in self.span.chars().rev() {
            n = n * base + i64::from(c.to_digit(radix).unwrap());
        }
        n
    }
}

#[derive(Clone, Debug)]
pub enum Sign {
    Plus { span: Span },
    Minus { span: Span },
}

impl Sign {
    fn parse(input: &mut Input) -> PResult<Self> {
        take(1usize)
            .verify_map(|span: Span| {
                match &*span {
                    "+" => Some(Self::Plus { span }),
                    "-" => Some(Self::Minus { span }),
                    _ => None,
                }
            })
            .parse_next(input)
    }

    fn signum(&self) -> i64 {
        match self {
            Self::Plus { .. } => 1,
            Self::Minus { .. } => -1,
        }
    }

    pub fn span(&self) -> &Span {
        match self {
            Sign::Plus { span } => span,
            Sign::Minus { span } => span,
        }
    }
}

#[derive(Clone, Debug)]
pub enum IntegerPrefix {
    Hex { span: Span },
    Oct { span: Span },
    Bin { span: Span },
}

impl IntegerPrefix {
    fn parse(input: &mut Input) -> PResult<Self> {
        take(2usize)
            .verify_map(|span: Span| {
                match &*span {
                    "0x" => Some(Self::Hex { span }),
                    "0o" => Some(Self::Oct { span }),
                    "0b" => Some(Self::Bin { span }),
                    _ => None,
                }
            })
            .parse_next(input)
    }

    fn base(&self) -> u8 {
        match self {
            IntegerPrefix::Hex { .. } => 16,
            IntegerPrefix::Oct { .. } => 8,
            IntegerPrefix::Bin { .. } => 2,
        }
    }
}

pub enum Float {
    Normal {
        span: Span,
        sign: Option<Sign>,
        int: Digits,
        frac: Option<Frac>,
        exp: Option<Exp>,
    },
    Inf {
        span: Span,
        sign: Option<Sign>,
        inf: Span,
    },
    Nan {
        span: Span,
        sign: Option<Sign>,
        nan: Span,
    },
}

pub struct Frac {
    dot: Span,
    digits: Digits,
}

pub struct Exp {
    e: Span,
    sign: Option<Sign>,
    exp: Digits,
}

impl Float {
    fn parse(input: &mut Input) -> PResult<Self> {
        alt((
            (opt(Sign::parse), "inf")
                .spanned()
                .map(move |(span, (sign, inf))| Self::Inf { span, sign, inf }),
            (opt(Sign::parse), "nan")
                .spanned()
                .map(move |(span, (sign, nan))| Self::Nan { span, sign, nan }),
            (
                opt(Sign::parse),
                Digits::parse_dec,
                opt((".", Digits::with_leading_zeros(10))),
                opt(("e", opt(Sign::parse), Digits::with_leading_zeros(10))),
            )
                .spanned()
                .map(move |(span, (sign, int, frac, exp))| {
                    Self::Normal {
                        span,
                        sign,
                        int,
                        frac: frac.map(|(dot, digits)| Frac { dot, digits }),
                        exp: exp.map(|(e, sign, exp)| Exp { e, sign, exp }),
                    }
                }),
        ))
        .parse_next(input)
    }
}

#[derive(Clone)]
pub enum Boolean {
    False { span: Span },
    True { span: Span },
}

impl Boolean {
    fn parse(input: &mut Input) -> PResult<Self> {
        alt((
            "false".map(|span| Self::False { span }),
            "true".map(|span| Self::True { span }),
        ))
        .parse_next(input)
    }
}

pub struct DateTime {
    span: Span,
    date: Option<Date>,
    time: Option<Time>,
}

impl DateTime {
    /// ```plain
    /// date-time      = offset-date-time / local-date-time / local-date / local-time
    ///
    /// date-fullyear  = 4DIGIT
    /// date-month     = 2DIGIT  ; 01-12
    /// date-mday      = 2DIGIT  ; 01-28, 01-29, 01-30, 01-31 based on month/year
    /// time-delim     = "T" / %x20 ; T, t, or space
    /// time-hour      = 2DIGIT  ; 00-23
    /// time-minute    = 2DIGIT  ; 00-59
    /// time-second    = 2DIGIT  ; 00-58, 00-59, 00-60 based on leap second rules
    /// time-secfrac   = "." 1*DIGIT
    /// time-numoffset = ( "+" / "-" ) time-hour ":" time-minute
    /// time-offset    = "Z" / time-numoffset
    ///
    /// partial-time   = time-hour ":" time-minute ":" time-second [ time-secfrac ]
    /// full-date      = date-fullyear "-" date-month "-" date-mday
    /// full-time      = partial-time time-offset
    ///
    /// offset-date-time = full-date time-delim full-time
    /// local-date-time = full-date time-delim partial-time
    /// local-date = full-date
    /// local-time = partial-time
    /// ```
    fn parse(input: &mut Input) -> PResult<Self> {
        todo!();
    }
}

pub struct Date {
    span: Span,
    year: Span,
    month: Span,
    day: Span,
}

pub struct Time {
    span: Span,
    hour: Span,
    minute: Span,
    second: Span,
    second_frac: Option<Span>,
    offset: Option<TimeOffset>,
}

pub enum TimeOffset {
    Z {
        span: Span,
    },
    Fixed {
        span: Span,
        sign: Sign,
        hour: Span,
        minute: Span,
    },
}

pub struct Table {
    span: Span,
}

impl Table {
    fn parse(input: &mut Input) -> PResult<Self> {
        todo!();
    }
}

pub struct Array {
    span: Span,
}

impl Array {
    fn parse(input: &mut Input) -> PResult<Self> {
        todo!();
    }
}

pub struct InlineTable {
    span: Span,
}

impl InlineTable {
    fn parse(input: &mut Input) -> PResult<Self> {
        todo!();
    }
}

pub struct Separated<Item, Sep> {
    items: Vec<Item>,
    separators: Vec<Sep>,
}

impl<Item, Sep> Separated<Item, Sep> {
    pub fn parser<Input, ItemParser, SepParser, Error>(
        occurences: impl Into<Range>,
        mut item_parser: ItemParser,
        mut separator_parser: SepParser,
    ) -> impl Parser<Input, Self, Error>
    where
        Input: Stream,
        ItemParser: Parser<Input, Item, Error>,
        SepParser: Parser<Input, Sep, Error>,
        Error: ParserError<Input>,
    {
        let range = occurences.into();
        let start = match range.start_bound() {
            Bound::Included(n) => *n,
            Bound::Excluded(n) => *n + 1,
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(n) => Some(*n + 1),
            Bound::Excluded(n) => Some(*n),
            Bound::Unbounded => None,
        };

        trace("separated", move |input: &mut Input| {
            let mut items = vec![];
            let mut separators = vec![];
            let mut n = 0;
            let checkpoint = input.checkpoint();

            match item_parser.parse_next(input) {
                Ok(item) => {
                    items.push(item);
                    n += 1;

                    while end.map_or(true, |end| n < end) {
                        let checkpoint = input.checkpoint();

                        match separator_parser.parse_next(input) {
                            Ok(separator) => {
                                separators.push(separator);
                                items.push(item_parser.parse_next(input)?);
                                n += 1;
                            }
                            Err(ErrMode::Backtrack(_)) if n >= start => {
                                input.reset(&checkpoint);
                                break;
                            }
                            Err(e) => return Err(e),
                        }
                    }
                }
                Err(ErrMode::Backtrack(_)) if start == 0 => {
                    input.reset(&checkpoint);
                }
                Err(e) => return Err(e),
            }

            Ok(Self { items, separators })
        })
    }
}

pub struct Delimited<Item, Delimiter> {
    start: Delimiter,
    item: Item,
    end: Delimiter,
}

impl<Item, Delimiter> Delimited<Item, Delimiter> {
    pub fn parser<Input, ItemParser, DelimiterParser, Error>(
        mut item_parser: ItemParser,
        mut delimiter_parser: DelimiterParser,
    ) -> impl Parser<Input, Self, Error>
    where
        Input: Stream,
        ItemParser: Parser<Input, Item, Error>,
        DelimiterParser: Parser<Input, Delimiter, Error>,
        Error: ParserError<Input>,
    {
        trace("delimited", move |input: &mut Input| {
            let start = delimiter_parser.parse_next(input)?;
            let item = item_parser.parse_next(input)?;
            let end = delimiter_parser.parse_next(input)?;

            Ok(Self { start, item, end })
        })
    }
}
