/// a ∨ b ∨ ... ∨ z
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Or<V, D = ()> {
    pub ands: Vec<And<V, D>>,
    pub data: D,
}

impl<V, D> Or<V, D> {
    pub fn map_data<E>(self, mut f: impl FnMut(D) -> E) -> Or<V, E> {
        Or {
            ands: self
                .ands
                .into_iter()
                .map(|and| and.map_data(&mut f))
                .collect(),
            data: f(self.data),
        }
    }
}

/// a ∧ b ∧ ... ∧ z
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct And<V, D = ()> {
    pub terms: Vec<Term<V, D>>,
    pub data: D,
}

impl<V, D> And<V, D> {
    pub fn map_data<E>(self, mut f: impl FnMut(D) -> E) -> And<V, E> {
        And {
            terms: self
                .terms
                .into_iter()
                .map(|and| and.map_data(&mut f))
                .collect(),
            data: f(self.data),
        }
    }
}

/// a, ¬a
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Term<V, D = ()> {
    pub negative: bool,
    pub expression: Expression<V, D>,
    pub data: D,
}

impl<V, D> Term<V, D> {
    pub fn map_data<E>(self, mut f: impl FnMut(D) -> E) -> Term<V, E> {
        Term {
            negative: self.negative,
            expression: self.expression.map_data(&mut f),
            data: f(self.data),
        }
    }
}

/// a, (...)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Expression<V, D = ()> {
    Or(Or<V, D>),
    Variable(Variable<V, D>),
}

impl<V, D> Expression<V, D> {
    pub fn map_data<E>(self, f: impl FnMut(D) -> E) -> Expression<V, E> {
        match self {
            Expression::Or(or) => {
                Expression::Or(or.map_data(Box::new(f) as Box<dyn FnMut(D) -> E>))
            }
            Expression::Variable(variable) => Expression::Variable(variable.map_data(f)),
        }
    }
}

/// a
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Variable<V, D = ()> {
    pub variable: V,
    pub data: D,
}

impl<V, D> Variable<V, D> {
    pub fn map_data<E>(self, mut f: impl FnMut(D) -> E) -> Variable<V, E> {
        Variable {
            variable: self.variable,
            data: f(self.data),
        }
    }
}
