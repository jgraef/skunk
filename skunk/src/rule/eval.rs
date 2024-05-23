use std::{
    any::{
        Any,
        TypeId,
    },
    collections::HashMap,
    hash::{
        DefaultHasher,
        Hash,
        Hasher,
    },
    ops::Deref,
    sync::{
        Arc,
        RwLock,
    },
};

use crate::util::boolean::{
    Maybe,
    VariableId,
};

#[derive(Debug)]
struct Inputs {
    inputs: HashMap<TypeId, Box<dyn Any>>,
}

impl Inputs {
    pub fn add<E, M>(
        &mut self,
        extractor: E,
        matcher: M,
        create_variable: impl FnOnce() -> VariableId,
    ) -> VariableId
    where
        E: Extractor + Eq + Hash + 'static,
        M: Match<E> + Eq + Hash + 'static,
    {
        let set = self
            .inputs
            .entry(extractor.type_id())
            .or_insert_with(|| Box::<InputSet<E>>::new(InputSet::default()))
            .downcast_mut::<InputSet<E>>()
            .unwrap();

        set.insert(extractor, matcher, create_variable)
    }

    pub fn input_set<E>(&self) -> Option<&InputSet<E>>
    where
        E: Extractor + 'static,
    {
        self.inputs
            .get(&TypeId::of::<E>())
            .map(|set| set.downcast_ref::<InputSet<E>>().unwrap())
    }
}

pub struct Input<E: Extractor> {
    extractor: E,
    matcher: Box<dyn Match<E>>,
    hash: u64,
    variable: VariableId,
}

impl<E: Extractor> Input<E> {
    pub fn extractor(&self) -> &E {
        &self.extractor
    }

    pub fn matcher(&self) -> &dyn Match<E> {
        &self.matcher
    }

    pub fn variable_id(&self) -> VariableId {
        self.variable
    }
}

struct InputSet<E: Extractor> {
    inputs: hashbrown::HashTable<Input<E>>,
}

impl<E: Extractor> Default for InputSet<E> {
    fn default() -> Self {
        Self {
            inputs: Default::default(),
        }
    }
}

impl<E> InputSet<E>
where
    E: Extractor,
{
    fn insert<M>(
        &mut self,
        extractor: E,
        matcher: M,
        create_variable: impl FnOnce() -> VariableId,
    ) -> VariableId
    where
        M: Match<E> + Eq + Hash + 'static,
        E: Eq + Hash + 'static,
    {
        let mut hasher = DefaultHasher::new();
        extractor.hash(&mut hasher);
        matcher.hash(&mut hasher);
        let hash = hasher.finish();

        match self.inputs.entry(
            hash,
            |other| {
                extractor == other.extractor && {
                    // does this try to downcast the Box? then this would always return false
                    (&other.matcher as &dyn Any)
                        .downcast_ref::<M>()
                        .map_or(false, |other| &matcher == other)
                }
            },
            |var| var.hash,
        ) {
            hashbrown::hash_table::Entry::Occupied(entry) => entry.get().variable,
            hashbrown::hash_table::Entry::Vacant(entry) => {
                let variable = create_variable();
                entry.insert(Input {
                    extractor,
                    matcher: Box::new(matcher),
                    hash,
                    variable,
                });
                variable
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Input<E>> {
        self.inputs.iter()
    }
}

pub trait Extractor {
    type Data<'d>;
}

pub trait Match<E: Extractor> {
    fn matches<'d>(&self, input: E::Data<'d>) -> Maybe;
}

impl<E: Extractor> Match<E> for Box<dyn Match<E>> {
    fn matches<'d>(&self, input: E::Data<'d>) -> Maybe {
        self.deref().matches(input)
    }
}

impl<E, F> Match<E> for F
where
    E: Extractor,
    for<'d> F: Fn(E::Data<'d>) -> Maybe,
{
    fn matches<'d>(&self, input: E::Data<'d>) -> Maybe {
        self(input)
    }
}

#[derive(Clone, Debug)]
pub struct Evaluator {
    expressions: Arc<RwLock<Expressions>>,
    values: Values,
}

impl Evaluator {
    pub fn set<E, F>(&mut self) -> SetVariables
    where
        E: Extractor,
    {
        SetVariables {
            expressions: self.expressions.read().unwrap(),
            values: &mut self.values,
        }
    }
}

pub struct SetVariables<'a> {
    expressions: RwLockReadGuard<'a, Expressions>,
    values: &'a mut Values,
}

impl<'a> SetVariables<'a> {
    pub fn for_each<E>(&mut self, f: impl Fn(&E, &dyn Match<E>) -> Maybe)
    where
        E: Extractor + 'static,
    {
        if let Some(set) = self.expressions.get_variable_set::<E>() {
            for var in set.iter() {
                let value = f(&var.extractor, &var.matcher);
                self.values.set(var.node_index, value, &self.expressions);
            }
        }
    }
}
