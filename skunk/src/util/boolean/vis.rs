use std::{
    fmt::Display,
    io::Write,
};

use petgraph::visit::IntoNodeReferences;

use super::{
    Graph,
    Node,
    NodeKind,
};

impl Graph {
    /// Writes the graph to `writer` as a [graphviz dot file][1]. This requires
    /// the `graph-vis` feature to be enabled.
    ///
    /// [1]: https://graphviz.org/doc/info/lang.html
    pub fn write_dot(&self, mut writer: impl Write) -> Result<(), std::io::Error> {
        writeln!(writer, "strict digraph {{")?;

        for (node_index, node) in self.graph.node_references() {
            writeln!(
                writer,
                "  {} [label=<{}>];",
                node_index.index(),
                NodeLabel(node),
            )?;
        }

        for edge_index in self.graph.edge_indices() {
            let edge = self.graph.edge_endpoints(edge_index).unwrap();
            writeln!(writer, "  {} -> {};", edge.0.index(), edge.1.index(),)?;
        }

        write!(writer, "}}")?;

        Ok(())
    }
}

/// Helper struct to format node labels
struct NodeLabel<'a>(&'a Node);

impl<'a> Display for NodeLabel<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0.kind {
            NodeKind::Literal(false) => write!(f, "&#8869;"),
            NodeKind::Literal(true) => write!(f, "&#8868;"),
            NodeKind::Variable => {
                if let Some(label) = &self.0.label {
                    write!(f, "input: {label}")
                }
                else {
                    write!(f, "input")
                }
            }
            NodeKind::Not => write!(f, "&#172;"),
            NodeKind::And => write!(f, "&#8743;"),
            NodeKind::Or => write!(f, "&#8744;"),
        }
    }
}
