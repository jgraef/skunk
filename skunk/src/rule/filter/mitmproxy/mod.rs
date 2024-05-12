use std::{
    hash::Hash,
    str::FromStr,
};

use crate::util::bool_expr::{
    ContinuousEvaluation,
    Or,
};

mod parser;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    Request,
    Response,
    Both,
}

// todo: this is suboptimal, but we need Eq + Hash
// todo: probably mve this up, since we might need this in other places as well.
#[derive(Clone, Debug)]
pub struct Regex {
    s: String,
    parsed: regex::Regex,
}

#[derive(Debug, thiserror::Error)]
#[error("regex parse error")]
pub struct RegexParseError(#[from] regex::Error);

impl FromStr for Regex {
    type Err = RegexParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.to_owned().try_into()
    }
}

impl TryFrom<String> for Regex {
    type Error = RegexParseError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let parsed = s.parse()?;
        Ok(Self { s: s, parsed })
    }
}

impl PartialEq for Regex {
    fn eq(&self, other: &Self) -> bool {
        self.s == other.s
    }
}

impl Eq for Regex {}

impl Hash for Regex {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.s.hash(state);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Filter {
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
}

#[derive(Clone, Debug)]
pub struct FilterExpression {
    expression: Or<Filter, ()>,
}

// todo: i don't know how we can get a VerboseError that is 'static. maybe
// ouroboros can help?
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ParseError(String);

impl FromStr for FilterExpression {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (_rest, expression) = self::parser::parse(s).map_err(|e| ParseError(e.to_string()))?;
        Ok(FilterExpression { expression })
    }
}

impl FilterExpression {
    pub fn begin_evaluate(&self) -> Evaluator {
        Evaluator {
            evaluator: ContinuousEvaluation::new(self.expression.clone()),
        }
    }
}

#[derive(Debug)]
pub struct Evaluator {
    evaluator: ContinuousEvaluation<Filter, ()>,
}

impl Evaluator {
    pub fn get(&self) -> Option<bool> {
        self.evaluator.get().into()
    }

    pub fn push<'f>(
        &mut self,
        assignments: impl IntoIterator<Item = (&'f Filter, bool)>,
    ) -> Option<bool> {
        self.evaluator.push(assignments).into()
    }
}
