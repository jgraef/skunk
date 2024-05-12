use nom::{
    branch::alt,
    bytes::complete::{
        escaped_transform,
        is_not,
        tag,
    },
    character::complete::{
        char,
        multispace0,
        multispace1,
    },
    combinator::{
        all_consuming,
        map,
        map_res,
        opt,
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
        tuple,
    },
    IResult,
};
use regex::Regex;

use super::Filter;
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
                        parse_filter_variant("a", tuple(()), |()| Filter::Asset),
                        parse_filter_variant("all", tuple(()), |()| Filter::All),
                        // todo: all filter variants
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
