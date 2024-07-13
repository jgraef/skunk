use std::{
    collections::HashMap,
    num::NonZeroUsize,
};

use petgraph::{
    graph::NodeIndex,
    Direction,
};

use super::{
    ExpressionId,
    Graph,
    Maybe,
    VariableId,
};
use crate::util::boolean::NodeKind;

#[derive(Clone, Debug)]
struct ExpressionState {
    value: Maybe,
    num_inputs_not_evaluated: usize,
}

// note: this takes `Graph` on creation and each `set` call. This means the
// graph can change inbetween. But that is fine, since already created
// expressions can't be modified.
//
// todo: document the behaviour when nodes are deleted in the graph.
#[derive(Clone, Debug)]
pub struct Evaluator {
    instance_id: NonZeroUsize,
    values: HashMap<NodeIndex, ExpressionState>,
}

impl Evaluator {
    pub(super) fn new(graph: &Graph) -> Self {
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
        propagate(graph.literals[0], false, graph, &mut values);
        propagate(graph.literals[1], true, graph, &mut values);

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
