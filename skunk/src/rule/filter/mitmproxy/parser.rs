use nom::{
    branch::alt,
    bytes::complete::{
        escaped_transform,
        is_not,
        tag,
    },
    character::complete::{
        char,
        digit1,
        multispace0,
        multispace1,
    },
    combinator::{
        all_consuming,
        map,
        map_res,
        opt,
        success,
        value,
    },
    error::{
        context,
        ErrorKind,
        FromExternalError,
        VerboseError,
    },
    multi::{
        many0_count,
        many1,
        separated_list0,
    },
    sequence::{
        delimited,
        pair,
        preceded,
        terminated,
    },
    IResult,
};

use super::{
    Direction,
    Filter,
    Regex,
};
use crate::util::bool_expr::{
    And,
    Expression,
    Or,
    Term,
    Variable,
};

type Res<'a, U> = IResult<&'a str, U, VerboseError<&'a str>>;

/// consumes a single comment
fn consume_comment(input: &str) -> Res<()> {
    value((), pair(char('#'), is_not("\r\n")))(input)
}

/// consumes whitespace and comments
fn consume_wsc<'a>(input: &'a str) -> Res<'a, ()> {
    value(
        (),
        terminated(
            many0_count(preceded(multispace0, consume_comment)),
            multispace0,
        ),
    )(input)
}

/// consumes whitespace and comments, but at least one whitespace
fn consume1_wsc<'a>(input: &'a str) -> Res<'a, ()> {
    value(
        (),
        terminated(
            many0_count(preceded(multispace0, consume_comment)),
            multispace1,
        ),
    )(input)
}

/// consumes all whitespace and comments before calling the parser `f`
fn wsc<'a, U>(f: impl FnMut(&'a str) -> Res<'a, U>) -> impl FnMut(&'a str) -> Res<'a, U> {
    preceded(consume_wsc, f)
}

pub fn parse(input: &str) -> Res<Or<Filter>> {
    all_consuming(terminated(parse_or, consume_wsc))(input)
}

fn parse_or(input: &str) -> Res<Or<Filter>> {
    context(
        "or",
        map(separated_list0(char('|'), parse_and), |ands| {
            Or { ands, data: () }
        }),
    )(input)
}

fn parse_and(input: &str) -> Res<And<Filter>> {
    context(
        "and",
        map(separated_list0(char('&'), many1(parse_term)), |terms| {
            And {
                terms: terms.into_iter().flatten().collect(),
                data: (),
            }
        }),
    )(input)
}

fn parse_term(input: &str) -> Res<Term<Filter>> {
    context(
        "term",
        map(
            pair(opt(char('!')), parse_expr),
            |(exclamation_mark, expression)| {
                Term {
                    negative: exclamation_mark.is_some(),
                    expression,
                    data: (),
                }
            },
        ),
    )(input)
}

/*
Asset,
    All,
    Body(Direction, Regex),
    HttpResponseCode(u16),
    Comment(Regex),
    Domain(Regex),
    Dns,
    Destination(Regex),
    Error,
    Header(Direction, Regex),
    Http,
    Method(Regex),
    Marked,
    Marker(Regex),
    Meta(Regex),
    Direction(Direction),
    Replay(Direction),
    Source(Regex),
    ContentType(Direction, Regex),
    Tcp,
    Url(Regex),
    Udp,
    Websocket,
*/

fn parse_expr(input: &str) -> Res<Expression<Filter>> {
    context(
        "expr",
        alt((
            map(delimited(wsc(char('(')), parse_or, wsc(char(')'))), |or| {
                Expression::Or(or)
            }),
            map(
                preceded(
                    wsc(char('~')),
                    alt((
                        alt((
                            parse_filter_variant("a", success(()), |()| Filter::Asset),
                            parse_filter_variant("all", success(()), |()| Filter::All),
                            parse_filter_variant("b", parse_regex, |regex| {
                                Filter::Body(Direction::Both, regex)
                            }),
                            parse_filter_variant("bq", parse_regex, |regex| {
                                Filter::Body(Direction::Request, regex)
                            }),
                            parse_filter_variant("bs", parse_regex, |regex| {
                                Filter::Body(Direction::Response, regex)
                            }),
                            parse_filter_variant("c", map_res(digit1, str::parse), |code| {
                                Filter::HttpResponseCode(code)
                            }),
                            parse_filter_variant("comment", parse_regex, |regex| {
                                Filter::Comment(regex)
                            }),
                            parse_filter_variant("d", parse_regex, |regex| Filter::Domain(regex)),
                            parse_filter_variant("dns", success(()), |()| Filter::Dns),
                            parse_filter_variant("dst", parse_regex, |regex| {
                                Filter::Destination(regex)
                            }),
                            parse_filter_variant("e", success(()), |()| Filter::Error),
                            parse_filter_variant("h", parse_regex, |regex| {
                                Filter::Header(Direction::Both, regex)
                            }),
                            parse_filter_variant("hq", parse_regex, |regex| {
                                Filter::Header(Direction::Request, regex)
                            }),
                            parse_filter_variant("hs", parse_regex, |regex| {
                                Filter::Header(Direction::Response, regex)
                            }),
                            parse_filter_variant("http", success(()), |()| Filter::Http),
                            parse_filter_variant("method", parse_regex, |regex| {
                                Filter::Method(regex)
                            }),
                            parse_filter_variant("marked", success(()), |()| Filter::Marked),
                            parse_filter_variant("marker", parse_regex, |regex| {
                                Filter::Marker(regex)
                            }),
                            parse_filter_variant("meta", parse_regex, |regex| Filter::Meta(regex)),
                            parse_filter_variant("q", success(()), |()| {
                                Filter::Direction(Direction::Request)
                            }),
                        )),
                        alt((
                            parse_filter_variant("replay", success(()), |()| {
                                Filter::Replay(Direction::Both)
                            }),
                            parse_filter_variant("replayq", success(()), |()| {
                                Filter::Replay(Direction::Request)
                            }),
                            parse_filter_variant("replays", success(()), |()| {
                                Filter::Replay(Direction::Response)
                            }),
                            parse_filter_variant("s", success(()), |()| {
                                Filter::Direction(Direction::Response)
                            }),
                            parse_filter_variant("src", parse_regex, |regex| Filter::Source(regex)),
                            parse_filter_variant("t", parse_regex, |regex| {
                                Filter::ContentType(Direction::Both, regex)
                            }),
                            parse_filter_variant("tq", parse_regex, |regex| {
                                Filter::ContentType(Direction::Request, regex)
                            }),
                            parse_filter_variant("ts", parse_regex, |regex| {
                                Filter::ContentType(Direction::Response, regex)
                            }),
                            parse_filter_variant("tcp", success(()), |()| Filter::Tcp),
                            parse_filter_variant("u", parse_regex, |regex| Filter::Url(regex)),
                            parse_filter_variant("udp", success(()), |()| Filter::Udp),
                            parse_filter_variant("websocket", success(()), |()| Filter::Websocket),
                        )),
                    )),
                ),
                |filter| {
                    Expression::Variable(Variable {
                        variable: filter,
                        data: (),
                    })
                },
            ),
        )),
    )(input)
}

fn parse_filter_variant<'a, U>(
    filter_tag: &'a str,
    args: impl FnMut(&'a str) -> Res<'a, U>,
    kind: impl FnMut(U) -> Filter,
) -> impl FnMut(&'a str) -> Res<'a, Filter> {
    map(preceded(tag(filter_tag), args), kind)
}

fn parse_regex(input: &str) -> Res<Regex> {
    context(
        "regex",
        map_res(
            wsc(delimited(
                char('"'),
                escaped_transform(
                    is_not("\\\""),
                    '\\',
                    alt((
                        value("\\", tag("\\")),
                        value("\"", tag("\"")),
                        // todo: regex escapes
                    )),
                ),
                char('"'),
            )),
            |regex| {
                regex
                    .parse()
                    .map_err(|e| VerboseError::from_external_error(input, ErrorKind::Fail, e))
            },
        ),
    )(input)
}
