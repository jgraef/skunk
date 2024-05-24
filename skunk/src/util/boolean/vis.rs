use std::io::Write;

pub use graphviz_rust::dot_structures as dot;
use graphviz_rust::printer::PrinterContext;
use petgraph::{
    graph::NodeIndex,
    visit::IntoNodeReferences,
};

use super::{
    Graph,
    Node,
    NodeKind,
};

fn node_id(node_index: NodeIndex) -> dot::NodeId {
    dot::NodeId(dot::Id::Anonymous(format!("n{}", node_index.index())), None)
}

fn node_label(node: &Node) -> dot::Id {
    dot::Id::Html(match node.kind {
        NodeKind::Literal(false) => "&#8869;".to_owned(),
        NodeKind::Literal(true) => "&#8868;".to_owned(),
        NodeKind::Variable => {
            node.label
                .as_ref()
                .map_or_else(|| "input".to_owned(), |label| format!("input: {label}"))
        }
        NodeKind::Not => "&#172;".to_owned(),
        NodeKind::And => "&#8743;".to_owned(),
        NodeKind::Or => "&#8744;".to_owned(),
    })
}

impl Graph {
    pub fn to_dot_graph(&self, graph_id: dot::Id) -> dot::Graph {
        let mut stmts = vec![];

        for (node_index, node) in self.graph.node_references() {
            stmts.push(dot::Stmt::Node(dot::Node {
                id: node_id(node_index),
                attributes: vec![dot::Attribute(
                    dot::Id::Plain("label".to_owned()),
                    node_label(node),
                )],
            }));
        }

        for edge_index in self.graph.edge_indices() {
            let edge = self.graph.edge_endpoints(edge_index).unwrap();
            stmts.push(dot::Stmt::Edge(dot::Edge {
                ty: dot::EdgeTy::Pair(
                    dot::Vertex::N(node_id(edge.0)),
                    dot::Vertex::N(node_id(edge.1)),
                ),
                attributes: vec![],
            }));
        }

        dot::Graph::DiGraph {
            id: graph_id,
            strict: true,
            stmts,
        }
    }

    pub fn write_dot(&self, mut writer: impl Write) -> Result<(), std::io::Error> {
        let graph = self.to_dot_graph(dot::Id::Anonymous("graph".to_owned()));
        let mut context = PrinterContext::default();
        let graph = graphviz_rust::print(graph, &mut context);
        writer.write_all(graph.as_bytes())?;
        Ok(())
    }
}
