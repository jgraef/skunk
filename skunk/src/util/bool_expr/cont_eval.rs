use std::{
    collections::HashMap,
    hash::Hash,
    ops::BitXor,
};

use super::expr::Or;
use crate::util::bool_expr::expr::{
    And,
    Expression,
    Term,
    Variable,
};

#[derive(Clone, Copy, Debug)]
pub enum Value {
    True,
    False,
    Unknown,
}

impl BitXor<bool> for Value {
    type Output = Value;

    fn bitxor(self, rhs: bool) -> Self::Output {
        match (self, rhs) {
            (Value::False, false) | (Value::True, true) => Value::False,
            (Value::False, true) | (Value::True, false) => Value::True,
            (Value::Unknown, _) => Value::Unknown,
        }
    }
}

impl From<Value> for Option<bool> {
    fn from(value: Value) -> Self {
        match value {
            Value::True => Some(true),
            Value::False => Some(false),
            Value::Unknown => None,
        }
    }
}

#[derive(Clone, Debug)]
struct Data<D> {
    data: D,
    value: Value,
}

#[derive(Clone, Debug)]
pub struct ContinuousEvaluation<V, D = ()> {
    expression: Or<V, Data<D>>,
}

impl<V, D> ContinuousEvaluation<V, D> {
    pub fn new(expression: Or<V, D>) -> Self {
        Self {
            expression: expression.map_data(|data| {
                Data {
                    data,
                    value: Value::Unknown,
                }
            }),
        }
    }

    pub fn get(&self) -> Value {
        self.expression.data.value
    }

    pub fn push<'v>(&mut self, assignments: impl IntoIterator<Item = (&'v V, bool)>) -> Value
    where
        V: Eq + Hash + 'v,
    {
        fn update_or<V: Eq + Hash, D>(or: &mut Or<V, Data<D>>, assignments: &HashMap<&V, bool>) {
            match or.data.value {
                Value::True => {}
                Value::False => {}
                Value::Unknown => {
                    let mut has_unknowns = false;
                    for and in &mut or.ands {
                        update_and(and, assignments);
                        match and.data.value {
                            Value::True => or.data.value = Value::True,
                            Value::Unknown => has_unknowns = true,
                            _ => {}
                        }
                    }
                    if !has_unknowns && matches!(or.data.value, Value::Unknown) {
                        or.data.value = Value::False;
                    }
                }
            }
        }

        fn update_and<V: Eq + Hash, D>(and: &mut And<V, Data<D>>, assignments: &HashMap<&V, bool>) {
            match and.data.value {
                Value::True => {}
                Value::False => {}
                Value::Unknown => {
                    let mut has_unknowns = false;
                    for term in &mut and.terms {
                        update_term(term, assignments);
                        match and.data.value {
                            Value::False => and.data.value = Value::False,
                            Value::Unknown => has_unknowns = true,
                            _ => {}
                        }
                    }
                    if !has_unknowns && matches!(and.data.value, Value::Unknown) {
                        and.data.value = Value::True;
                    }
                }
            }
        }

        fn update_term<V: Eq + Hash, D>(
            term: &mut Term<V, Data<D>>,
            assignments: &HashMap<&V, bool>,
        ) {
            match term.data.value {
                Value::True => {}
                Value::False => {}
                Value::Unknown => {
                    match &mut term.expression {
                        Expression::Or(or) => {
                            update_or(or, assignments);
                            term.data.value = or.data.value ^ term.negative;
                        }
                        Expression::Variable(variable) => {
                            update_variable(variable, assignments);
                            term.data.value = variable.data.value ^ term.negative;
                        }
                    }
                }
            }
        }

        fn update_variable<V: Eq + Hash, D>(
            variable: &mut Variable<V, Data<D>>,
            assignments: &HashMap<&V, bool>,
        ) {
            match variable.data.value {
                Value::True => {}
                Value::False => {}
                Value::Unknown => {
                    match assignments.get(&variable.variable) {
                        Some(false) => variable.data.value = Value::False,
                        Some(true) => variable.data.value = Value::True,
                        _ => {}
                    }
                }
            }
        }

        let assignments = assignments.into_iter().collect();
        update_or(&mut self.expression, &assignments);

        self.expression.data.value
    }
}
