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
    ops::{
        Deref,
        DerefMut,
    },
    sync::Arc,
};

use parking_lot::{
    lock_api::ArcRwLockWriteGuard,
    RawRwLock,
    RwLock,
    RwLockReadGuard,
};

use crate::util::boolean::{
    self,
    ExpressionId,
    Maybe,
    VariableId,
};

#[derive(Debug)]
pub struct Builder {
    inner: GraphInner,
}

impl Builder {
    #[inline]
    pub fn literal(&mut self, value: bool) -> ExpressionId {
        self.inner.graph.literal(value)
    }

    #[inline]
    pub fn input<E, M>(&mut self, extractor: E, matcher: M) -> VariableId
    where
        E: Extractor + Eq + Hash + 'static,
        M: Match<E> + Eq + Hash + 'static,
    {
        self.inner
            .inputs
            .add(extractor, matcher, || self.inner.graph.variable())
    }

    #[inline]
    pub fn not(&mut self, input: ExpressionId) -> ExpressionId {
        self.inner.graph.not(input)
    }

    #[inline]
    pub fn and(&mut self, inputs: &[ExpressionId]) -> ExpressionId {
        self.inner.graph.and(inputs)
    }

    #[inline]
    pub fn or(&mut self, inputs: &[ExpressionId]) -> ExpressionId {
        self.inner.graph.or(inputs)
    }

    #[inline]
    pub fn build(self) -> Graph {
        Graph {
            inner: Arc::new(RwLock::new(Arc::new(RwLock::new(self.inner)))),
        }
    }

    #[inline]
    pub fn replace(self, graph: &Graph) {
        graph.replace(self)
    }
}

pub struct Modify {
    inner: ArcRwLockWriteGuard<RawRwLock, GraphInner>,
}

impl Modify {
    #[inline]
    pub fn literal(&mut self, value: bool) -> ExpressionId {
        self.inner.graph.literal(value)
    }

    #[inline]
    pub fn input<E, M>(&mut self, extractor: E, matcher: M) -> VariableId
    where
        E: Extractor + Eq + Hash + 'static,
        M: Match<E> + Eq + Hash + 'static,
    {
        let inner = self.inner.deref_mut();
        inner
            .inputs
            .add(extractor, matcher, || inner.graph.variable())
    }

    #[inline]
    pub fn not(&mut self, input: ExpressionId) -> ExpressionId {
        self.inner.graph.not(input)
    }

    #[inline]
    pub fn and(&mut self, inputs: &[ExpressionId]) -> ExpressionId {
        self.inner.graph.and(inputs)
    }

    #[inline]
    pub fn or(&mut self, inputs: &[ExpressionId]) -> ExpressionId {
        self.inner.graph.or(inputs)
    }
}

#[derive(Debug)]
struct GraphInner {
    graph: boolean::Graph,
    inputs: Inputs,
}

#[derive(Clone)]
pub struct Graph {
    // this nestedness of `Arc`s and `RwLock` is intentional. We want to share this graph. But we
    // can also swap out the inner `Arc`. The inner `Arc` is then shared with `Evaluator`s.
    inner: Arc<RwLock<Arc<RwLock<GraphInner>>>>,
}

impl Graph {
    #[inline]
    pub fn modify(&self) -> Modify {
        Modify {
            inner: self.inner.read().write_arc(),
        }
    }

    #[inline]
    pub fn replace(&self, builder: Builder) {
        let mut inner = self.inner.write();
        *inner = Arc::new(RwLock::new(builder.inner));
    }

    pub fn evaluator(&self) -> Evaluator {
        let inner = self.inner.read().clone();
        let eval = inner.read().graph.evaluator();
        Evaluator { eval, inner }
    }
}

#[derive(Clone, Debug)]
pub struct Evaluator {
    eval: boolean::Evaluator,
    inner: Arc<RwLock<GraphInner>>,
}

impl Evaluator {
    pub fn update<E, F>(&mut self) -> UpdateInputs
    where
        E: Extractor,
    {
        UpdateInputs {
            eval: &mut self.eval,
            inner: self.inner.read(),
        }
    }
}

pub struct UpdateInputs<'a> {
    eval: &'a mut boolean::Evaluator,
    inner: RwLockReadGuard<'a, GraphInner>,
}

impl<'a> UpdateInputs<'a> {
    pub fn for_each<'d, E, F>(&mut self, f: F)
    where
        E: Extractor + 'static,
        F: Fn(&E) -> E::Data<'d>,
    {
        if let Some(set) = self.inner.inputs.input_set::<E>() {
            for var in set.iter() {
                let data = f(&var.extractor);
                if let Maybe::Definite(value) = var.matcher.matches(data) {
                    self.eval.set(&self.inner.graph, var.variable, value);
                }
            }
        }
    }
}

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
