use std::str::FromStr;

use crate::util::bool_expr::{
    ContinuousEvaluation,
    Or,
};

mod parser;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Filter {
    Asset,
    All,
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
