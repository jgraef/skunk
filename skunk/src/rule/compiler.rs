use std::borrow::Cow;

use super::file::{
    Block,
    Condition,
    Conditions,
    Rule,
    SubCondition,
};
use crate::util::boolean::{
    ExpressionId,
    ModifyGraph,
};

#[derive(Debug, thiserror::Error)]
pub enum Error<B: Backend + ?Sized> {
    #[error("backend error")]
    Backend(#[source] B::Error),

    #[error("the filter {name} requires user interaction")]
    RequiresUserInteraction { name: Cow<'static, str> },
}

#[derive(Debug, Default)]
pub struct Config {
    pub with_user_interaction: bool,
}

#[derive(Debug)]
pub struct Compiler<'a, B> {
    config: &'a Config,
    backend: &'a mut B,
}

impl<'a, B> Compiler<'a, B>
where
    B: Backend,
{
    pub fn new(config: &'a Config, backend: &'a mut B) -> Self {
        Self { config, backend }
    }

    pub fn compile_block(
        &mut self,
        block: &Block<B::Filter, B::Effect>,
        condition: Option<ExpressionId>,
        parent_scope: Option<&mut B::Scope>,
    ) -> Result<(), Error<B>> {
        let mut scope = self.backend.scope(parent_scope, condition);

        for rule in &block.rules {
            self.compile_rule(rule, condition, &mut scope)?;
        }

        for effect in &block.effects {
            self.backend.compile_effect(&mut scope, effect)?;
        }

        Ok(())
    }

    pub fn compile_rule(
        &mut self,
        rule: &Rule<B::Filter, B::Effect>,
        condition: Option<ExpressionId>,
        scope: &mut B::Scope,
    ) -> Result<(), Error<B>> {
        if !rule.then.is_empty() || rule.alt.is_empty() {
            let mut cond_expressions =
                Vec::with_capacity(rule.condition.0.len() + condition.map_or(0, |_| 1));
            cond_expressions.extend(condition);
            self.compile_conditions_into(&rule.condition, &mut cond_expressions, scope)?;
            let then_cond = self.backend.and(&cond_expressions);

            if !rule.then.is_empty() {
                self.compile_block(&rule.then, Some(then_cond), Some(scope))?;
            }

            if !rule.alt.is_empty() {
                let alt_cond = self.backend.not(then_cond);
                self.compile_block(&rule.alt, Some(alt_cond), Some(scope))?;
            }
        }

        Ok(())
    }

    pub fn compile_conditions_into(
        &mut self,
        conditions: &Conditions<B::Filter>,
        expressions: &mut Vec<ExpressionId>,
        scope: &mut B::Scope,
    ) -> Result<(), Error<B>> {
        expressions.reserve(expressions.len());
        for condition in &conditions.0 {
            expressions.push(self.compile_condition(condition, scope)?);
        }
        Ok(())
    }

    pub fn compile_conditions_with<O>(
        &mut self,
        conditions: &Conditions<B::Filter>,
        op: O,
        scope: &mut B::Scope,
    ) -> Result<ExpressionId, Error<B>>
    where
        O: FnOnce(&mut B, &[ExpressionId]) -> ExpressionId,
    {
        let mut expressions = vec![];
        self.compile_conditions_into(conditions, &mut expressions, scope)?;
        Ok(op(self.backend, &expressions))
    }

    pub fn compile_condition(
        &mut self,
        condition: &Condition<B::Filter>,
        scope: &mut B::Scope,
    ) -> Result<ExpressionId, Error<B>> {
        let expression = match condition {
            Condition::Sub(SubCondition::Not(not)) => {
                // implicit and, then not
                let and_expr = self.compile_conditions_with(not, ModifyGraph::and, scope)?;
                self.backend.not(and_expr)
            }
            Condition::Sub(SubCondition::And(and)) => {
                // explicit and
                self.compile_conditions_with(and, ModifyGraph::and, scope)?
            }
            Condition::Sub(SubCondition::Or(or)) => {
                // explicit or
                self.compile_conditions_with(or, ModifyGraph::or, scope)?
            }
            Condition::Terminal(terminal) => self.backend.compile_filter(scope, terminal)?,
        };
        Ok(expression)
    }
}

pub trait Backend: ModifyGraph {
    type Filter;
    type Effect;
    type Scope;
    type Error: std::error::Error + 'static;

    fn scope(
        &mut self,
        parent: Option<&mut Self::Scope>,
        condition: Option<ExpressionId>,
    ) -> Self::Scope;

    fn compile_filter(
        &mut self,
        scope: &mut Self::Scope,
        filter: &Self::Filter,
    ) -> Result<ExpressionId, Error<Self>>;

    fn compile_effect(
        &mut self,
        scope: &mut Self::Scope,
        effect: &Self::Effect,
    ) -> Result<(), Error<Self>>;
}
