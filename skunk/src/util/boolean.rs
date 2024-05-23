use std::{
    collections::{
        HashMap,
        HashSet,
    },
    fmt::{
        Debug,
        Display,
    },
    hash::Hash,
    sync::atomic::{
        AtomicUsize,
        Ordering,
    },
};

use petgraph::{
    graph::NodeIndex,
    stable_graph::StableGraph,
    Direction,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expression {
    Literal(bool),
    Variable,
    Not,
    And,
    Or,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExpressionId {
    instance_id: usize,
    node_index: NodeIndex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VariableId(ExpressionId);

impl From<VariableId> for ExpressionId {
    fn from(value: VariableId) -> Self {
        value.0
    }
}

pub struct ExpressionGraph {
    instance_id: usize,
    graph: StableGraph<Expression, ()>,
    literals: [NodeIndex; 2],
}

impl Default for ExpressionGraph {
    fn default() -> Self {
        static INSTANCE_ID: AtomicUsize = AtomicUsize::new(1);
        let instance_id = INSTANCE_ID.fetch_add(1, Ordering::Relaxed);

        let mut graph = StableGraph::with_capacity(2, 0);

        let literals = [
            graph.add_node(Expression::Literal(false)),
            graph.add_node(Expression::Literal(true)),
        ];

        Self {
            instance_id,
            graph,
            literals,
        }
    }
}

impl ExpressionGraph {
    #[inline]
    fn expression_id(&self, node_index: NodeIndex) -> ExpressionId {
        ExpressionId {
            instance_id: self.instance_id,
            node_index,
        }
    }

    #[inline]
    fn check_expression_id(&self, expression_id: &ExpressionId) {
        check_expression_id(self.instance_id, expression_id);
    }

    #[inline]
    pub fn literal(&self, value: bool) -> ExpressionId {
        self.expression_id(self.literals[usize::from(value)])
    }

    #[inline]
    pub fn variable<E, M>(&mut self) -> VariableId {
        let node_index = self.graph.add_node(Expression::Variable);
        VariableId(self.expression_id(node_index))
    }

    pub fn not(&mut self, input: ExpressionId) -> ExpressionId {
        self.check_expression_id(&input);

        for index in self
            .graph
            .neighbors_directed(input.node_index, Direction::Outgoing)
        {
            let node = self.graph.node_weight(index).unwrap();
            if matches!(node, Expression::Not) {
                return self.expression_id(index);
            }
        }

        let index = self.graph.add_node(Expression::Not);
        self.graph.add_edge(input.node_index, index, ());
        self.expression_id(index)
    }

    pub fn and(&mut self, inputs: &[ExpressionId]) -> ExpressionId {
        if inputs.is_empty() {
            self.literal(true)
        }
        else {
            self.binary(inputs, Expression::And)
        }
    }

    pub fn or(&mut self, inputs: &[ExpressionId]) -> ExpressionId {
        if inputs.is_empty() {
            self.literal(false)
        }
        else {
            self.binary(inputs, Expression::Or)
        }
    }

    pub fn evaluator(&self) -> Evaluator {
        let mut values = HashMap::new();

        values.insert(
            self.literals[0],
            ExpressionState {
                value: false.into(),
                num_inputs_not_evaluated: 0,
            },
        );
        values.insert(
            self.literals[1],
            ExpressionState {
                value: true.into(),
                num_inputs_not_evaluated: 0,
            },
        );

        Evaluator {
            instance_id: self.instance_id,
            values,
        }
    }

    fn binary(&mut self, inputs: &[ExpressionId], kind: Expression) -> ExpressionId {
        // note: this assumes there's at least 1 input, and that `kind` is either `And`
        // or `Or`.

        // to check whether this node already exists, we iterate over its inputs.
        // for each input we look at the outgoing edges, i.e. where it's used as input,
        // called *dependants*. we calculate the intersection of these input
        // dependants. we end up with either an empty set, meaning this node
        // doesn't exist yet, or with a set with one element, which is the equivalent
        // node.

        for input in inputs {
            self.check_expression_id(input);
        }

        // first, if the input consists of only 1 node, we can just return that.
        if inputs.len() == 1 {
            return inputs[0];
        }

        // helper to get the inputs' dependants filtered for the node kind we're looking
        // for.
        let dependants = |index| {
            self.graph
                .neighbors_directed(index, Direction::Outgoing)
                .filter(|index| *self.graph.node_weight(*index).unwrap() == kind)
        };

        // we initialize our intersection with the first input's dependants.
        let mut input_iter = inputs.iter();
        let mut dependants_intersection_1 =
            dependants(input_iter.next().expect("no inputs").node_index).collect::<HashSet<_>>();
        // instead of creating a new hash set for each intersection, we just use 2 and
        // swap them around.
        let mut dependants_intersection_2 = HashSet::with_capacity(dependants_intersection_1.len());

        // compute the intersection of the inputs' dependants
        // we always store the output of the intersection in set 2, and then swap. so
        // the overall result will be in set 1.
        for input in input_iter {
            dependants_intersection_2.clear();
            dependants_intersection_2.extend(
                dependants(input.node_index)
                    .filter(|index| dependants_intersection_1.contains(index)),
            );
            std::mem::swap(
                &mut dependants_intersection_1,
                &mut dependants_intersection_2,
            );
            // early break, if intersection is already empty
            if dependants_intersection_1.is_empty() {
                break;
            }
        }

        let index = match dependants_intersection_1.len() {
            0 => {
                // create a new node
                let index = self.graph.add_node(kind);
                for input in inputs {
                    self.graph.add_edge(input.node_index, index, ());
                }
                index
            }
            1 => {
                // we found an equivalent node
                *dependants_intersection_1.iter().next().unwrap()
            }
            _ => panic!("bug: found more than 1 equivalent node"),
        };

        self.expression_id(index)
    }
}

impl Debug for ExpressionGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExpressionGraph")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy)]
pub enum Maybe {
    Indefinite,
    Definite(bool),
}

impl Default for Maybe {
    fn default() -> Self {
        Self::Indefinite
    }
}

impl From<bool> for Maybe {
    fn from(value: bool) -> Self {
        Self::Definite(value)
    }
}

impl From<Option<bool>> for Maybe {
    fn from(value: Option<bool>) -> Self {
        value.map_or(Self::Indefinite, Self::Definite)
    }
}

impl From<Maybe> for Option<bool> {
    fn from(value: Maybe) -> Self {
        match value {
            Maybe::Definite(value) => Some(value),
            Maybe::Indefinite => None,
        }
    }
}

impl Display for Maybe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Indefinite => write!(f, "indefinite"),
            Self::Definite(value) => write!(f, "{value}"),
        }
    }
}

impl Debug for Maybe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}

#[derive(Clone, Debug)]
struct ExpressionState {
    value: Maybe,
    num_inputs_not_evaluated: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Evaluator {
    instance_id: usize,
    values: HashMap<NodeIndex, ExpressionState>,
}

impl Evaluator {
    pub fn set(&mut self, variable: VariableId, new_value: bool, graph: &ExpressionGraph) {
        if graph.instance_id != self.instance_id {
            tracing::error!(graph = %graph.instance_id, evaluator = %self.instance_id, "Use of foreign ExpressionGraph");
            panic!("Use of foreign ExpressionGraph");
        }
        let expression_id = variable.into();
        self.check_expression_id(&expression_id);
        let node_index = expression_id.node_index;

        let state = self.values.entry(node_index).or_insert_with(|| {
            ExpressionState {
                value: Maybe::Indefinite,
                num_inputs_not_evaluated: 0,
            }
        });

        match state.value {
            Maybe::Indefinite => {
                // update value
                state.value = Maybe::Definite(new_value);
                self.propagate(node_index, new_value, graph);
            }
            Maybe::Definite(old_value) if old_value != new_value => {
                // old value was already definite, but different
                panic!("Trying to assign a new value, but a different value is already set: old_value={old_value}, new_value={new_value}");
            }
            _ => {}
        }
    }

    pub fn get(&self, expression_id: ExpressionId) -> Maybe {
        self.check_expression_id(&expression_id);
        self.values
            .get(&expression_id.node_index)
            .map(|state| state.value)
            .unwrap_or_default()
    }

    #[inline]
    fn check_expression_id(&self, expression_id: &ExpressionId) {
        check_expression_id(self.instance_id, expression_id);
    }

    /// helper to propagate value recursively
    fn propagate(&mut self, node_index: NodeIndex, new_value: bool, graph: &ExpressionGraph) {
        for dependant_index in graph
            .graph
            .neighbors_directed(node_index, Direction::Outgoing)
        {
            let dependant_expression = graph.graph.node_weight(dependant_index).unwrap();
            let dependant_state = self.values.entry(dependant_index).or_insert_with(|| {
                let num_inputs = graph
                    .graph
                    .neighbors_directed(dependant_index, Direction::Incoming)
                    .count();
                assert!(!matches!(dependant_expression, Expression::Not) || num_inputs == 1);
                ExpressionState {
                    value: Maybe::Indefinite,
                    num_inputs_not_evaluated: num_inputs,
                }
            });

            dependant_state.num_inputs_not_evaluated -= 1;

            // compute new value
            let dependant_new_value = match (
                new_value,
                dependant_expression,
                dependant_state.num_inputs_not_evaluated,
            ) {
                (_, Expression::Not, _) => {
                    // not: just invert
                    Maybe::Definite(!new_value)
                }
                (false, Expression::And, _) => {
                    // and, but we received a false, so the output is false
                    Maybe::Definite(false)
                }
                (true, Expression::Or, _) => {
                    // or, but we received a true, so the output is true
                    Maybe::Definite(true)
                }
                (true, Expression::And, 0) => {
                    // and, all inputs evaluated, so it's true
                    Maybe::Definite(true)
                }
                (false, Expression::Or, 0) => {
                    // or, all inputs evaluated, so it's false
                    Maybe::Definite(false)
                }
                _ => Maybe::Indefinite,
            };

            // validate and propagate if needed
            match (dependant_state.value, dependant_new_value) {
                (Maybe::Indefinite, Maybe::Indefinite) => {
                    // indefinite -> indefinite: okay
                    // no propagation needed
                }
                (Maybe::Definite(old_value), Maybe::Definite(new_value))
                    if old_value == new_value =>
                {
                    // definite -> definite with same value: okay
                    // no propagation needed
                }
                (Maybe::Indefinite, Maybe::Definite(new_value)) => {
                    // indefinite -> definite: okay
                    // propagate
                    dependant_state.value = dependant_new_value;
                    self.propagate(dependant_index, new_value, graph);
                }
                _ => {
                    // invalid transition
                    panic!("invalid transition of expression value: old_value={}, new_value={dependant_new_value}", dependant_state.value);
                }
            }
        }
    }
}

#[inline]
fn check_expression_id(instance_id: usize, expression_id: &ExpressionId) {
    if expression_id.instance_id != instance_id {
        tracing::error!(?expression_id, %instance_id, "Use of foreign ExpressionId");
        panic!("Use of foreign ExpressionId");
    }
}
