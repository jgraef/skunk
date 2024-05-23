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
    num::NonZeroUsize,
};

use petgraph::{
    graph::NodeIndex,
    stable_graph::StableGraph,
    Direction,
};

use super::unique_ids;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Node {
    kind: NodeKind,
    pin_count: usize,
}

impl Node {
    #[inline]
    pub fn new(kind: NodeKind) -> Self {
        Self { kind, pin_count: 0 }
    }

    #[inline]
    pub fn literal(value: bool) -> Self {
        Self::new(NodeKind::Literal(value))
    }

    #[inline]
    pub fn variable() -> Self {
        Self::new(NodeKind::Variable)
    }

    #[inline]
    pub fn not() -> Self {
        Self::new(NodeKind::Not)
    }

    #[inline]
    pub fn and() -> Self {
        Self::new(NodeKind::And)
    }

    #[inline]
    pub fn or() -> Self {
        Self::new(NodeKind::Or)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NodeKind {
    Literal(bool),
    Variable,
    Not,
    And,
    Or,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub struct ExpressionId {
    instance_id: NonZeroUsize,
    node_index: NodeIndex,
}

impl ExpressionId {
    #[inline]
    fn expect_instance(&self, expected_instance_id: NonZeroUsize) {
        if self.instance_id != expected_instance_id {
            tracing::error!(expression_id = ?self, %expected_instance_id, "Use of foreign ExpressionId");
            panic!("Use of foreign ExpressionId");
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub struct VariableId(ExpressionId);

impl From<VariableId> for ExpressionId {
    fn from(value: VariableId) -> Self {
        value.0
    }
}

pub struct Graph {
    instance_id: NonZeroUsize,
    graph: StableGraph<Node, ()>,
    literals: [NodeIndex; 2],
}

impl Default for Graph {
    fn default() -> Self {
        let instance_id = unique_ids!();

        let mut graph = StableGraph::with_capacity(2, 0);

        let literals = [
            graph.add_node(Node::literal(false)),
            graph.add_node(Node::literal(true)),
        ];

        Self {
            instance_id,
            graph,
            literals,
        }
    }
}

impl Graph {
    #[inline]
    fn expression_id(&self, node_index: NodeIndex) -> ExpressionId {
        ExpressionId {
            instance_id: self.instance_id,
            node_index,
        }
    }

    #[inline]
    pub fn literal(&self, value: bool) -> ExpressionId {
        self.expression_id(self.literals[usize::from(value)])
    }

    #[inline]
    pub fn variable(&mut self) -> VariableId {
        let node_index = self.graph.add_node(Node::variable());
        VariableId(self.expression_id(node_index))
    }

    pub fn not(&mut self, input: ExpressionId) -> ExpressionId {
        input.expect_instance(self.instance_id);

        for index in self
            .graph
            .neighbors_directed(input.node_index, Direction::Outgoing)
        {
            let node = self.graph.node_weight(index).unwrap();
            if matches!(node.kind, NodeKind::Not) {
                return self.expression_id(index);
            }
        }

        let index = self.graph.add_node(Node::not());
        self.graph.add_edge(input.node_index, index, ());
        self.expression_id(index)
    }

    #[inline]
    pub fn and(&mut self, inputs: &[ExpressionId]) -> ExpressionId {
        if inputs.is_empty() {
            self.literal(true)
        }
        else {
            self.binary(inputs, NodeKind::And)
        }
    }

    #[inline]
    pub fn or(&mut self, inputs: &[ExpressionId]) -> ExpressionId {
        if inputs.is_empty() {
            self.literal(false)
        }
        else {
            self.binary(inputs, NodeKind::Or)
        }
    }

    #[inline]
    pub fn pin(&mut self, expression_id: ExpressionId) {
        expression_id.expect_instance(self.instance_id);
        self.graph
            .node_weight_mut(expression_id.node_index)
            .unwrap_or_else(|| panic!("missing expression: {expression_id:?}"))
            .pin_count += 1;
    }

    pub fn unpin(&mut self, expression_id: ExpressionId) {
        /// removes nodes without dependants recursively
        fn remove(graph: &mut StableGraph<Node, ()>, node_index: NodeIndex) {
            if graph
                .neighbors_directed(node_index, Direction::Outgoing)
                .count()
                == 0
            {
                let mut dependencies = graph
                    .neighbors_directed(node_index, Direction::Incoming)
                    .detach();
                while let Some(dependency_index) = dependencies.next_node(graph) {
                    if graph.node_weight(dependency_index).unwrap().pin_count == 0 {
                        remove(graph, dependency_index);
                    }
                }
                graph.remove_node(node_index);
            }
        }

        expression_id.expect_instance(self.instance_id);
        let Some(node) = self.graph.node_weight_mut(expression_id.node_index)
        else {
            return;
        };
        node.pin_count = node.pin_count.checked_sub(1).unwrap_or_default();
        if node.pin_count == 0 {
            remove(&mut self.graph, expression_id.node_index);
        }
    }

    #[inline]
    pub fn evaluator(&self) -> Evaluator {
        Evaluator::new(self)
    }

    fn binary(&mut self, inputs: &[ExpressionId], kind: NodeKind) -> ExpressionId {
        // note: this assumes there's at least 1 input, and that `kind` is either `And`
        // or `Or`.

        // to check whether this node already exists, we iterate over its inputs.
        // for each input we look at the outgoing edges, i.e. where it's used as input,
        // called *dependants*. we calculate the intersection of these input
        // dependants. we end up with either an empty set, meaning this node
        // doesn't exist yet, or with a set with one element, which is the equivalent
        // node.

        for input in inputs {
            input.expect_instance(self.instance_id);
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
                .filter(|index| self.graph.node_weight(*index).unwrap().kind == kind)
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
                let index = self.graph.add_node(Node::new(kind));
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

impl Debug for Graph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Graph")
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

// note: this takes `Graph` on creation and each `set` call. This means the
// graph can change inbetween. But that is fine, since already created
// expressions can't be modified.
#[derive(Clone, Debug)]
pub struct Evaluator {
    instance_id: NonZeroUsize,
    values: HashMap<NodeIndex, ExpressionState>,
}

impl Evaluator {
    fn new(graph: &Graph) -> Self {
        let mut values = HashMap::new();

        // add literals
        values.insert(
            graph.literals[0],
            ExpressionState {
                value: false.into(),
                num_inputs_not_evaluated: 0,
            },
        );
        values.insert(
            graph.literals[1],
            ExpressionState {
                value: true.into(),
                num_inputs_not_evaluated: 0,
            },
        );
        propagate(graph.literals[0], false, &graph, &mut values);
        propagate(graph.literals[1], true, &graph, &mut values);

        Self {
            instance_id: graph.instance_id,
            values,
        }
    }

    pub fn set(&mut self, graph: &Graph, variable: VariableId, new_value: bool) {
        if self.instance_id != graph.instance_id {
            panic!(
                "Use of foreign Graph: graph.instance_id={}, Evaluator.instance_id={}",
                graph.instance_id, self.instance_id
            );
        }
        let expression_id: ExpressionId = variable.into();
        expression_id.expect_instance(self.instance_id);
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
                propagate(node_index, new_value, graph, &mut self.values);
            }
            Maybe::Definite(old_value) if old_value != new_value => {
                // old value was already definite, but different
                panic!("Trying to assign a new value, but a different value is already set: old_value={old_value}, new_value={new_value}");
            }
            _ => {}
        }
    }

    pub fn get(&self, expression_id: ExpressionId) -> Maybe {
        expression_id.expect_instance(self.instance_id);

        self.values
            .get(&expression_id.node_index)
            .map(|state| state.value)
            .unwrap_or_default()
    }
}

/// helper to propagate value recursively
fn propagate(
    node_index: NodeIndex,
    new_value: bool,
    graph: &Graph,
    values: &mut HashMap<NodeIndex, ExpressionState>,
) {
    for dependant_index in graph
        .graph
        .neighbors_directed(node_index, Direction::Outgoing)
    {
        let dependant_expression = graph.graph.node_weight(dependant_index).unwrap();
        let dependant_state = values.entry(dependant_index).or_insert_with(|| {
            let num_inputs = graph
                .graph
                .neighbors_directed(dependant_index, Direction::Incoming)
                .count();
            assert!(!matches!(dependant_expression.kind, NodeKind::Not) || num_inputs == 1);
            ExpressionState {
                value: Maybe::Indefinite,
                num_inputs_not_evaluated: num_inputs,
            }
        });

        dependant_state
            .num_inputs_not_evaluated
            .checked_sub(1)
            .expect("node received more inputs than expected");

        // compute new value
        let dependant_new_value = match (
            new_value,
            dependant_expression.kind,
            dependant_state.num_inputs_not_evaluated,
        ) {
            (_, NodeKind::Not, _) => {
                // not: just invert
                Maybe::Definite(!new_value)
            }
            (false, NodeKind::And, _) => {
                // and, but we received a false, so the output is false
                Maybe::Definite(false)
            }
            (true, NodeKind::Or, _) => {
                // or, but we received a true, so the output is true
                Maybe::Definite(true)
            }
            (true, NodeKind::And, 0) => {
                // and, all inputs evaluated, so it's true
                Maybe::Definite(true)
            }
            (false, NodeKind::Or, 0) => {
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
            (Maybe::Definite(old_value), Maybe::Definite(new_value)) if old_value == new_value => {
                // definite -> definite with same value: okay
                // no propagation needed
            }
            (Maybe::Indefinite, Maybe::Definite(new_value)) => {
                // indefinite -> definite: okay
                // propagate
                dependant_state.value = dependant_new_value;
                propagate(dependant_index, new_value, graph, values);
            }
            _ => {
                // invalid transition
                panic!("invalid transition of expression value: old_value={}, new_value={dependant_new_value}", dependant_state.value);
            }
        }
    }
}
