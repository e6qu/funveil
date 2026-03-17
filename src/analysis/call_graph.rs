//! Call graph analysis for tracing function dependencies.
//!
//! This module builds a directed graph of function calls using petgraph,
//! enabling forward tracing (what functions does X call) and backward
//! tracing (what functions call X).

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::{IntoNeighbors, Reversed};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::parser::{CodeIndex, ParsedFile, Symbol};

/// Common Rust standard library and built-in function names to filter out
const STD_FUNCTIONS: &[&str] = &[
    // Result/Option methods
    "unwrap",
    "expect",
    "ok",
    "err",
    "map",
    "and_then",
    "or_else",
    "unwrap_or",
    "unwrap_or_else",
    "is_some",
    "is_none",
    "is_ok",
    "is_err",
    // Iterator methods
    "iter",
    "into_iter",
    "next",
    "map",
    "filter",
    "collect",
    "fold",
    "for_each",
    "count",
    "sum",
    "product",
    "any",
    "all",
    "find",
    "position",
    "enumerate",
    "zip",
    "chain",
    "take",
    "skip",
    "rev",
    // String methods
    "to_string",
    "to_owned",
    "clone",
    "as_str",
    "as_ref",
    "into",
    "from",
    "parse",
    "trim",
    "split",
    "join",
    "replace",
    "push",
    "push_str",
    "pop",
    "len",
    "is_empty",
    "contains",
    "starts_with",
    "ends_with",
    // Vec/Slice methods
    "push",
    "pop",
    "insert",
    "remove",
    "get",
    "first",
    "last",
    "sort",
    "reverse",
    "extend",
    "append",
    "clear",
    "resize",
    "truncate",
    // Path methods
    "join",
    "parent",
    "exists",
    "is_file",
    "is_dir",
    "file_name",
    "extension",
    "to_path_buf",
    "canonicalize",
    "read_dir",
    "components",
    // File/IO methods
    "read_to_string",
    "write",
    "read",
    "open",
    "create",
    "flush",
    // Other common methods
    "as_ref",
    "as_mut",
    "as_ptr",
    "as_slice",
    "to_vec",
    "to_bytes",
    "default",
    "new",
    "drop",
    "clone",
    "copy",
    "eq",
    "cmp",
    "partial_cmp",
    "to_os_string",
    "into_string",
    "display",
    "to_string_lossy",
    // Testing
    "assert",
    "assert_eq",
    "assert_ne",
    "panic",
    "print",
    "println",
    "eprint",
    "eprintln",
    "format",
    "write",
    "writeln",
    // Python builtins
    "str",
    "int",
    "float",
    "bool",
    "list",
    "dict",
    "set",
    "tuple",
    "type",
    "isinstance",
    "issubclass",
    "hasattr",
    "getattr",
    "setattr",
    "delattr",
    "range",
    "enumerate",
    "zip",
    "map",
    "filter",
    "sorted",
    "reversed",
    "min",
    "max",
    "abs",
    "round",
    "sum",
    "any",
    "all",
    "next",
    "iter",
    "id",
    "hash",
    "repr",
    "super",
    "property",
    "staticmethod",
    "classmethod",
    "object",
    "Exception",
    "ValueError",
    "TypeError",
    "KeyError",
    "IndexError",
    "AttributeError",
    "RuntimeError",
    "NotImplementedError",
    "StopIteration",
];

/// Check if a function name is likely a standard library function
fn is_std_function(name: &str) -> bool {
    if STD_FUNCTIONS.contains(&name) {
        return true;
    }

    // Filter out test artifacts from dependencies (test_0_XXX_N pattern)
    if name.starts_with("test_0_") {
        return true;
    }

    false
}

/// A node in the call graph representing a function
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FunctionNode {
    /// The fully qualified function name
    pub name: String,
    /// Optional file path where the function is defined
    pub file: Option<std::path::PathBuf>,
    /// Optional line number
    pub line: Option<usize>,
}

impl FunctionNode {
    /// Create a new function node
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            file: None,
            line: None,
        }
    }

    /// Create a new function node with location info
    pub fn with_location(name: impl Into<String>, file: std::path::PathBuf, line: usize) -> Self {
        Self {
            name: name.into(),
            file: Some(file),
            line: Some(line),
        }
    }
}

impl std::fmt::Display for FunctionNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// An edge in the call graph representing a call relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallEdge {
    /// The line number where the call occurs
    pub line: usize,
    /// Whether this is a dynamic call (function pointer, trait object, etc.)
    pub is_dynamic: bool,
}

/// Direction of call graph traversal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceDirection {
    /// Forward: find functions called by the starting function
    Forward,
    /// Backward: find functions that call the starting function
    Backward,
}

impl std::fmt::Display for TraceDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceDirection::Forward => write!(f, "forward"),
            TraceDirection::Backward => write!(f, "backward"),
        }
    }
}

/// Result of a trace operation
#[derive(Debug, Clone)]
pub struct TraceResult {
    /// The starting function
    pub start: FunctionNode,
    /// Direction of the trace
    pub direction: TraceDirection,
    /// Maximum depth searched
    pub depth: usize,
    /// Nodes found at each depth level
    pub levels: Vec<Vec<FunctionNode>>,
    /// Whether a cycle was detected
    pub cycle_detected: bool,
}

impl TraceResult {
    /// Get all unique functions found in the trace
    pub fn all_functions(&self) -> Vec<&FunctionNode> {
        let mut seen = HashSet::new();
        let mut result = Vec::new();

        for level in &self.levels {
            for node in level {
                if seen.insert(&node.name) {
                    result.push(node);
                }
            }
        }

        result
    }

    /// Filter out standard library functions from the result
    pub fn filter_std(&mut self) {
        for level in &mut self.levels {
            level.retain(|node| !is_std_function(&node.name));
        }
        self.levels.retain(|level| !level.is_empty());
    }

    /// Format the trace result as a tree
    pub fn format_tree(&self) -> String {
        let mut output = String::new();
        let dir_str = match self.direction {
            TraceDirection::Forward => "calls",
            TraceDirection::Backward => "is called by",
        };

        output.push_str(&format!("{} {}:\n", self.start.name, dir_str));

        for (depth, level) in self.levels.iter().enumerate() {
            let indent = "  ".repeat(depth + 1);
            for node in level {
                let connector = if depth == self.levels.len() - 1 && !self.cycle_detected {
                    "└──"
                } else {
                    "├──"
                };
                output.push_str(&format!("{}{} {}\n", indent, connector, node.name));
            }
        }

        if self.cycle_detected {
            output.push_str("\n(cycle detected - trace may be incomplete)\n");
        }

        output
    }

    /// Format the trace result as a flat list
    pub fn format_list(&self) -> String {
        let mut output = String::new();
        let dir_str = match self.direction {
            TraceDirection::Forward => "Called",
            TraceDirection::Backward => "Called by",
        };

        output.push_str(&format!("{}: {}\n", dir_str, self.start.name));
        output.push_str(&"=".repeat(40));
        output.push('\n');

        for (depth, level) in self.levels.iter().enumerate() {
            if !level.is_empty() {
                output.push_str(&format!("\n[Depth {}]\n", depth + 1));
                for node in level {
                    output.push_str(&format!("  - {}\n", node.name));
                }
            }
        }

        if self.cycle_detected {
            output.push_str("\n(cycle detected - trace may be incomplete)\n");
        }

        output
    }
}

/// Call graph builder and analyzer
#[derive(Debug, Clone)]
pub struct CallGraph {
    /// The underlying directed graph
    graph: DiGraph<FunctionNode, CallEdge>,
    /// Map from function name to node index
    name_to_index: HashMap<String, NodeIndex>,
    /// Set of function names in the graph
    functions: HashSet<String>,
}

impl Default for CallGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl CallGraph {
    /// Create a new empty call graph
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            name_to_index: HashMap::new(),
            functions: HashSet::new(),
        }
    }

    /// Add a function node to the graph
    pub fn add_function(&mut self, node: FunctionNode) -> NodeIndex {
        if let Some(&idx) = self.name_to_index.get(&node.name) {
            // Update existing node with more info if available
            if let Some(existing) = self.graph.node_weight_mut(idx) {
                if existing.file.is_none() && node.file.is_some() {
                    existing.file = node.file;
                    existing.line = node.line;
                }
            }
            idx
        } else {
            let name = node.name.clone();
            let idx = self.graph.add_node(node);
            self.name_to_index.insert(name.clone(), idx);
            self.functions.insert(name);
            idx
        }
    }

    /// Add a call edge from caller to callee
    pub fn add_call(&mut self, caller: &str, callee: &str, edge: CallEdge) {
        let caller_node = FunctionNode::new(caller);
        let callee_node = FunctionNode::new(callee);

        let caller_idx = self.add_function(caller_node);
        let callee_idx = self.add_function(callee_node);

        self.graph.add_edge(caller_idx, callee_idx, edge);
    }

    /// Get a node by function name
    pub fn get_node(&self, name: &str) -> Option<&FunctionNode> {
        self.name_to_index
            .get(name)
            .and_then(|&idx| self.graph.node_weight(idx))
    }

    /// Check if the graph contains a function
    pub fn contains(&self, name: &str) -> bool {
        self.functions.contains(name)
    }

    /// Get the number of functions in the graph
    pub fn function_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Get the number of call edges in the graph
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Trace function calls in the specified direction
    pub fn trace(
        &self,
        start: &str,
        direction: TraceDirection,
        max_depth: usize,
    ) -> Option<TraceResult> {
        let start_idx = *self.name_to_index.get(start)?;
        let start_node = self.graph.node_weight(start_idx)?.clone();

        let mut visited = HashSet::new();
        let mut levels: Vec<Vec<FunctionNode>> = Vec::new();
        let mut cycle_detected = false;

        let mut current_level = vec![start_idx];
        visited.insert(start_idx);

        for _depth in 0..max_depth {
            if current_level.is_empty() {
                break;
            }

            let mut next_level = Vec::new();
            let mut level_nodes = Vec::new();

            for node_idx in &current_level {
                let neighbors: Vec<NodeIndex> = match direction {
                    TraceDirection::Forward => self.graph.neighbors(*node_idx).collect(),
                    TraceDirection::Backward => {
                        let reversed = Reversed(&self.graph);
                        reversed.neighbors(*node_idx).collect()
                    }
                };

                for neighbor_idx in neighbors {
                    if let Some(node) = self.graph.node_weight(neighbor_idx) {
                        if !visited.contains(&neighbor_idx) {
                            visited.insert(neighbor_idx);
                            next_level.push(neighbor_idx);
                            level_nodes.push(node.clone());
                        } else {
                            // Node was already visited - potential cycle
                            cycle_detected = true;
                        }
                    }
                }
            }

            if !level_nodes.is_empty() {
                levels.push(level_nodes);
            }
            current_level = next_level;
        }

        Some(TraceResult {
            start: start_node,
            direction,
            depth: max_depth,
            levels,
            cycle_detected,
        })
    }

    /// Get immediate callees of a function (functions called by this function)
    pub fn callees(&self, function: &str) -> Vec<&FunctionNode> {
        let idx = match self.name_to_index.get(function) {
            Some(&idx) => idx,
            None => return Vec::new(),
        };

        self.graph
            .neighbors(idx)
            .filter_map(|neighbor_idx| self.graph.node_weight(neighbor_idx))
            .collect()
    }

    /// Get immediate callers of a function (functions that call this function)
    pub fn callers(&self, function: &str) -> Vec<&FunctionNode> {
        let idx = match self.name_to_index.get(function) {
            Some(&idx) => idx,
            None => return Vec::new(),
        };

        self.graph
            .neighbors_directed(idx, petgraph::Direction::Incoming)
            .filter_map(|neighbor_idx| self.graph.node_weight(neighbor_idx))
            .collect()
    }

    /// Export the graph in DOT format for visualization
    pub fn to_dot(&self) -> String {
        use std::fmt::Write;

        let mut output = String::new();
        writeln!(output, "digraph CallGraph {{").unwrap();
        writeln!(output, "    rankdir=LR;").unwrap();
        writeln!(output, "    node [shape=box];").unwrap();

        for node in self.graph.node_weights() {
            let escaped_name = node.name.replace('"', "\\\"");
            writeln!(output, "    \"{escaped_name}\" [label=\"{escaped_name}\"];").unwrap();
        }

        for edge in self.graph.edge_indices() {
            let (source, target) = self.graph.edge_endpoints(edge).unwrap();
            let source_node = self.graph.node_weight(source).unwrap();
            let target_node = self.graph.node_weight(target).unwrap();
            writeln!(
                output,
                "    \"{}\" -> \"{}\";",
                source_node.name.replace('"', "\\\""),
                target_node.name.replace('"', "\\\"")
            )
            .unwrap();
        }

        writeln!(output, "}}").unwrap();
        output
    }

    /// Filter out standard library functions from the graph
    pub fn filter_std_functions(&mut self) {
        let mut std_nodes: Vec<NodeIndex> = self
            .graph
            .node_indices()
            .filter(|idx| {
                self.graph
                    .node_weight(*idx)
                    .map(|n| is_std_function(&n.name))
                    .unwrap_or(false)
            })
            .collect();

        // Sort in descending index order so swap-remove doesn't invalidate pending indices
        std_nodes.sort_by_key(|b| std::cmp::Reverse(b.index()));

        for idx in std_nodes {
            if let Some(node) = self.graph.node_weight(idx) {
                self.name_to_index.remove(&node.name);
                self.functions.remove(&node.name);
            }
            self.graph.remove_node(idx);
        }

        // Rebuild name_to_index since indices may have shifted
        self.name_to_index.clear();
        for idx in self.graph.node_indices() {
            if let Some(node) = self.graph.node_weight(idx) {
                self.name_to_index.insert(node.name.clone(), idx);
            }
        }
    }
}

/// Build a call graph from parsed files
pub struct CallGraphBuilder;

impl CallGraphBuilder {
    /// Build a call graph from a collection of parsed files
    pub fn from_files(files: &[ParsedFile]) -> CallGraph {
        let mut graph = CallGraph::new();

        for file in files {
            for symbol in &file.symbols {
                if let Symbol::Function {
                    name, line_range, ..
                } = symbol
                {
                    let node = FunctionNode::with_location(
                        name.clone(),
                        file.path.clone(),
                        line_range.start(),
                    );
                    graph.add_function(node);
                }
            }
        }

        for file in files {
            for call in &file.calls {
                if let Some(ref caller) = call.caller {
                    graph.add_call(
                        caller,
                        &call.callee,
                        CallEdge {
                            line: call.line,
                            is_dynamic: call.is_dynamic,
                        },
                    );
                } else {
                    // Add callee as external function (no known caller)
                    graph.add_function(FunctionNode::new(&call.callee));
                }
            }
        }

        graph
    }

    /// Build a call graph from a CodeIndex
    pub fn from_index(index: &CodeIndex) -> CallGraph {
        let files: Vec<_> = index.files.values().cloned().collect();
        Self::from_files(&files)
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_test_call_graph() -> CallGraph {
        let mut graph = CallGraph::new();

        // Build a simple call graph:
        // main -> process -> helper
        //      -> validate
        graph.add_call(
            "main",
            "process",
            CallEdge {
                line: 10,
                is_dynamic: false,
            },
        );
        graph.add_call(
            "main",
            "validate",
            CallEdge {
                line: 11,
                is_dynamic: false,
            },
        );
        graph.add_call(
            "process",
            "helper",
            CallEdge {
                line: 20,
                is_dynamic: false,
            },
        );

        graph
    }

    #[test]
    fn test_is_std_function_known() {
        assert!(is_std_function("unwrap"));
        assert!(is_std_function("expect"));
        assert!(is_std_function("clone"));
        assert!(is_std_function("to_string"));
        assert!(is_std_function("collect"));
        assert!(is_std_function("parse"));
        assert!(is_std_function("len"));
        assert!(is_std_function("is_empty"));
        assert!(is_std_function("push"));
    }

    #[test]
    fn test_is_std_function_prefixes() {
        // Only names in the exact STD_FUNCTIONS list should match
        assert!(is_std_function("as_str"));
        assert!(is_std_function("to_vec"));
        assert!(is_std_function("is_some"));
    }

    #[test]
    fn test_is_std_function_does_not_filter_user_functions() {
        assert!(!is_std_function("get_users"));
        assert!(!is_std_function("set_config"));
        assert!(!is_std_function("new_connection"));
        assert!(!is_std_function("is_valid"));
        assert!(!is_std_function("has_permission"));
    }

    #[test]
    fn test_is_std_function_test_pattern() {
        assert!(is_std_function("test_0_crash_test"));
    }

    #[test]
    fn test_is_std_function_not_std() {
        assert!(!is_std_function("my_custom_function"));
        assert!(!is_std_function("process_data"));
        assert!(!is_std_function("MyFunction"));
        assert!(!is_std_function("mod::function_name"));
    }

    #[test]
    fn test_function_node_new() {
        let node = FunctionNode::new("my_func");
        assert_eq!(node.name, "my_func");
        assert!(node.file.is_none());
        assert!(node.line.is_none());
    }

    #[test]
    fn test_function_node_with_location() {
        let node = FunctionNode::with_location("my_func", PathBuf::from("test.rs"), 42);
        assert_eq!(node.name, "my_func");
        assert_eq!(node.file, Some(PathBuf::from("test.rs")));
        assert_eq!(node.line, Some(42));
    }

    #[test]
    fn test_function_node_display() {
        let node = FunctionNode::new("my_func");
        assert_eq!(format!("{node}"), "my_func");
    }

    #[test]
    fn test_trace_direction_display() {
        assert_eq!(format!("{}", TraceDirection::Forward), "forward");
        assert_eq!(format!("{}", TraceDirection::Backward), "backward");
    }

    #[test]
    fn test_trace_result_all_functions() {
        let graph = create_test_call_graph();
        let result = graph.trace("main", TraceDirection::Forward, 3).unwrap();

        let all_funcs = result.all_functions();
        assert!(!all_funcs.is_empty());

        let names: Vec<_> = all_funcs.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"process"));
        assert!(names.contains(&"validate"));
        assert!(names.contains(&"helper"));
    }

    #[test]
    fn test_trace_result_filter_std() {
        let mut graph = CallGraph::new();
        graph.add_call(
            "main",
            "unwrap",
            CallEdge {
                line: 1,
                is_dynamic: false,
            },
        );
        graph.add_call(
            "main",
            "my_func",
            CallEdge {
                line: 2,
                is_dynamic: false,
            },
        );

        let mut result = graph.trace("main", TraceDirection::Forward, 2).unwrap();
        assert!(result.levels[0].len() == 2);

        result.filter_std();
        assert!(result.levels[0].len() == 1);
        assert_eq!(result.levels[0][0].name, "my_func");
    }

    #[test]
    fn test_trace_result_format_tree() {
        let graph = create_test_call_graph();
        let result = graph.trace("main", TraceDirection::Forward, 2).unwrap();

        let tree = result.format_tree();
        assert!(tree.contains("main calls"));
        assert!(tree.contains("process"));
    }

    #[test]
    fn test_trace_result_format_tree_backward() {
        let graph = create_test_call_graph();
        let result = graph.trace("helper", TraceDirection::Backward, 2).unwrap();

        let tree = result.format_tree();
        assert!(tree.contains("helper is called by"));
        assert!(tree.contains("process"));
    }

    #[test]
    fn test_trace_result_format_tree_with_cycle() {
        let mut graph = CallGraph::new();
        graph.add_call(
            "a",
            "b",
            CallEdge {
                line: 1,
                is_dynamic: false,
            },
        );
        graph.add_call(
            "b",
            "a",
            CallEdge {
                line: 2,
                is_dynamic: false,
            },
        );

        let result = graph.trace("a", TraceDirection::Forward, 5).unwrap();
        let tree = result.format_tree();

        if result.cycle_detected {
            assert!(tree.contains("cycle detected"));
        }
    }

    #[test]
    fn test_trace_result_format_list() {
        let graph = create_test_call_graph();
        let result = graph.trace("main", TraceDirection::Forward, 2).unwrap();

        let list = result.format_list();
        assert!(list.contains("Called:"));
        assert!(list.contains("[Depth 1]"));
    }

    #[test]
    fn test_trace_result_format_list_backward() {
        let graph = create_test_call_graph();
        let result = graph.trace("helper", TraceDirection::Backward, 2).unwrap();

        let list = result.format_list();
        assert!(list.contains("Called by:"));
    }

    #[test]
    fn test_call_graph_default() {
        let graph = CallGraph::default();
        assert_eq!(graph.function_count(), 0);
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn test_add_function_updates_existing() {
        let mut graph = CallGraph::new();

        graph.add_function(FunctionNode::new("my_func"));
        graph.add_function(FunctionNode::with_location(
            "my_func",
            PathBuf::from("test.rs"),
            10,
        ));

        let node = graph.get_node("my_func").unwrap();
        assert_eq!(node.file, Some(PathBuf::from("test.rs")));
        assert_eq!(node.line, Some(10));
        assert_eq!(graph.function_count(), 1);
    }

    #[test]
    fn test_get_node() {
        let graph = create_test_call_graph();

        let node = graph.get_node("main").unwrap();
        assert_eq!(node.name, "main");

        assert!(graph.get_node("nonexistent").is_none());
    }

    #[test]
    fn test_call_graph_construction() {
        let graph = create_test_call_graph();

        assert_eq!(graph.function_count(), 4);
        assert_eq!(graph.edge_count(), 3);

        assert!(graph.contains("main"));
        assert!(graph.contains("process"));
        assert!(graph.contains("helper"));
        assert!(graph.contains("validate"));
    }

    #[test]
    fn test_callees() {
        let graph = create_test_call_graph();

        let main_callees: Vec<_> = graph
            .callees("main")
            .iter()
            .map(|n| n.name.clone())
            .collect();
        assert!(main_callees.contains(&"process".to_string()));
        assert!(main_callees.contains(&"validate".to_string()));
        assert_eq!(main_callees.len(), 2);

        let process_callees: Vec<_> = graph
            .callees("process")
            .iter()
            .map(|n| n.name.clone())
            .collect();
        assert!(process_callees.contains(&"helper".to_string()));
        assert_eq!(process_callees.len(), 1);
    }

    #[test]
    fn test_callers() {
        let graph = create_test_call_graph();

        let process_callers: Vec<_> = graph
            .callers("process")
            .iter()
            .map(|n| n.name.clone())
            .collect();
        assert!(process_callers.contains(&"main".to_string()));
        assert_eq!(process_callers.len(), 1);

        let helper_callers: Vec<_> = graph
            .callers("helper")
            .iter()
            .map(|n| n.name.clone())
            .collect();
        assert!(helper_callers.contains(&"process".to_string()));
        assert_eq!(helper_callers.len(), 1);
    }

    #[test]
    fn test_trace_forward() {
        let graph = create_test_call_graph();

        let result = graph.trace("main", TraceDirection::Forward, 3).unwrap();

        assert_eq!(result.start.name, "main");
        assert_eq!(result.direction, TraceDirection::Forward);
        assert!(!result.cycle_detected);

        // Level 0: process, validate
        assert_eq!(result.levels[0].len(), 2);
        let names: Vec<_> = result.levels[0].iter().map(|n| n.name.clone()).collect();
        assert!(names.contains(&"process".to_string()));
        assert!(names.contains(&"validate".to_string()));

        // Level 1: helper
        assert_eq!(result.levels[1].len(), 1);
        assert_eq!(result.levels[1][0].name, "helper");
    }

    #[test]
    fn test_trace_backward() {
        let graph = create_test_call_graph();

        let result = graph.trace("helper", TraceDirection::Backward, 3).unwrap();

        assert_eq!(result.start.name, "helper");
        assert_eq!(result.direction, TraceDirection::Backward);

        // Level 0: process
        assert_eq!(result.levels[0].len(), 1);
        assert_eq!(result.levels[0][0].name, "process");

        // Level 1: main
        assert_eq!(result.levels[1].len(), 1);
        assert_eq!(result.levels[1][0].name, "main");
    }

    #[test]
    fn test_trace_with_cycle() {
        let mut graph = CallGraph::new();

        // Create a cycle: a -> b -> c -> a
        graph.add_call(
            "a",
            "b",
            CallEdge {
                line: 1,
                is_dynamic: false,
            },
        );
        graph.add_call(
            "b",
            "c",
            CallEdge {
                line: 2,
                is_dynamic: false,
            },
        );
        graph.add_call(
            "c",
            "a",
            CallEdge {
                line: 3,
                is_dynamic: false,
            },
        );

        let result = graph.trace("a", TraceDirection::Forward, 5).unwrap();

        assert!(result.cycle_detected);
        // Should stop at reasonable depth despite the cycle
        assert!(result.levels.len() <= 3);
    }

    #[test]
    fn test_trace_depth_limit() {
        let graph = create_test_call_graph();

        // Trace with depth 1
        let result = graph.trace("main", TraceDirection::Forward, 1).unwrap();
        assert_eq!(result.levels.len(), 1);
        // BUG-059: depth limit should NOT falsely report a cycle
        assert!(!result.cycle_detected);
    }

    #[test]
    fn test_to_dot() {
        let graph = create_test_call_graph();
        let dot = graph.to_dot();

        assert!(dot.contains("digraph CallGraph"));
        assert!(dot.contains("\"main\""));
        assert!(dot.contains("\"process\""));
        assert!(dot.contains("\"main\" -> \"process\""));
    }

    #[test]
    fn test_to_dot_escapes_quotes() {
        let mut graph = CallGraph::new();
        graph.add_function(FunctionNode::new("func\"with\"quotes"));

        let dot = graph.to_dot();
        assert!(dot.contains("func\\\"with\\\"quotes"));
    }

    #[test]
    fn test_filter_std_functions() {
        let mut graph = CallGraph::new();
        graph.add_call(
            "main",
            "unwrap",
            CallEdge {
                line: 1,
                is_dynamic: false,
            },
        );
        graph.add_call(
            "main",
            "clone",
            CallEdge {
                line: 2,
                is_dynamic: false,
            },
        );
        graph.add_call(
            "main",
            "my_func",
            CallEdge {
                line: 3,
                is_dynamic: false,
            },
        );

        assert_eq!(graph.function_count(), 4);
        assert_eq!(graph.edge_count(), 3);

        graph.filter_std_functions();

        assert_eq!(graph.function_count(), 2);
        assert!(graph.contains("main"));
        assert!(graph.contains("my_func"));
        assert!(!graph.contains("unwrap"));
        assert!(!graph.contains("clone"));
    }

    #[test]
    fn test_nonexistent_function() {
        let graph = create_test_call_graph();

        assert!(graph
            .trace("nonexistent", TraceDirection::Forward, 3)
            .is_none());
        assert!(graph.callees("nonexistent").is_empty());
        assert!(graph.callers("nonexistent").is_empty());
    }

    #[test]
    fn test_trace_result_formats() {
        let graph = create_test_call_graph();
        let result = graph.trace("main", TraceDirection::Forward, 2).unwrap();

        let tree = result.format_tree();
        assert!(tree.contains("main"));
        assert!(tree.contains("process"));
        assert!(tree.contains("helper"));

        let list = result.format_list();
        assert!(list.contains("main"));
        assert!(list.contains("process"));
        assert!(list.contains("Called"));
    }

    #[test]
    fn test_call_graph_builder_from_files() {
        use crate::parser::{Call, Language, ParsedFile, Visibility};
        use crate::types::LineRange;

        let mut file = ParsedFile::new(Language::Rust, PathBuf::from("test.rs"));
        file.symbols.push(Symbol::Function {
            name: "main".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(2, 5).unwrap(),
            is_async: false,
            attributes: vec![],
        });
        file.calls.push(Call {
            caller: Some("main".to_string()),
            callee: "helper".to_string(),
            line: 3,
            is_dynamic: false,
        });

        let graph = CallGraphBuilder::from_files(&[file]);

        assert!(graph.contains("main"));
        assert!(graph.contains("helper"));
    }

    #[test]
    fn test_call_graph_builder_from_files_external_call() {
        use crate::parser::{Call, Language, ParsedFile};

        let mut file = ParsedFile::new(Language::Rust, PathBuf::from("test.rs"));
        file.calls.push(Call {
            caller: None,
            callee: "external_func".to_string(),
            line: 1,
            is_dynamic: false,
        });

        let graph = CallGraphBuilder::from_files(&[file]);

        assert!(graph.contains("external_func"));
    }

    #[test]
    fn test_call_graph_builder_from_index() {
        use crate::parser::{CodeIndex, Language, ParsedFile, Symbol, Visibility};
        use crate::types::LineRange;

        let mut files = std::collections::HashMap::new();
        let mut file = ParsedFile::new(Language::Rust, PathBuf::from("test.rs"));
        file.symbols.push(Symbol::Function {
            name: "main".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(2, 5).unwrap(),
            is_async: false,
            attributes: vec![],
        });
        files.insert(PathBuf::from("test.rs"), file);

        let index = CodeIndex::build(files);
        let graph = CallGraphBuilder::from_index(&index);

        assert!(graph.contains("main"));
    }

    #[test]
    fn test_trace_tree_last_level_connector() {
        let graph = create_test_call_graph();
        let result = graph.trace("main", TraceDirection::Forward, 2).unwrap();

        let tree = result.format_tree();
        assert!(tree.contains("└──") || tree.contains("├──"));
    }

    #[test]
    fn test_filter_std_functions_removes_edges() {
        use crate::parser::{Call, Language, ParsedFile, Symbol, Visibility};
        use crate::types::LineRange;

        let mut file = ParsedFile::new(Language::Rust, PathBuf::from("test.rs"));
        file.symbols.push(Symbol::Function {
            name: "main".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(2, 5).unwrap(),
            is_async: false,
            attributes: vec![],
        });
        file.calls.push(Call {
            caller: Some("main".to_string()),
            callee: "unwrap".to_string(),
            line: 3,
            is_dynamic: false,
        });

        let mut graph = CallGraphBuilder::from_files(&[file]);
        assert!(graph.contains("unwrap"));

        graph.filter_std_functions();
        assert!(!graph.contains("unwrap"));
        assert!(graph.callers("unwrap").is_empty());
    }

    #[test]
    fn test_trace_tree_connector_both_branches() {
        // Create a graph with multiple levels so we exercise both
        // the "└──" (last level, no cycle) and "├──" (non-last level) connectors.
        let mut graph = CallGraph::new();

        // a -> b -> c (3 levels: depth 0=a, level0=[b], level1=[c])
        graph.add_call(
            "a",
            "b",
            CallEdge {
                line: 1,
                is_dynamic: false,
            },
        );
        graph.add_call(
            "b",
            "c",
            CallEdge {
                line: 2,
                is_dynamic: false,
            },
        );

        let result = graph.trace("a", TraceDirection::Forward, 3).unwrap();
        assert!(!result.cycle_detected);
        assert!(result.levels.len() >= 2);

        let tree = result.format_tree();
        // First level should use "├──" (not last level)
        assert!(tree.contains("├──"));
        // Last level should use "└──" (last level, no cycle)
        assert!(tree.contains("└──"));
    }

    #[test]
    fn test_format_list_with_cycle() {
        let mut graph = CallGraph::new();
        graph.add_call(
            "a",
            "b",
            CallEdge {
                line: 1,
                is_dynamic: false,
            },
        );
        graph.add_call(
            "b",
            "a",
            CallEdge {
                line: 2,
                is_dynamic: false,
            },
        );

        let result = graph.trace("a", TraceDirection::Forward, 5).unwrap();
        let list = result.format_list();
        if result.cycle_detected {
            assert!(list.contains("cycle detected"));
        }
    }

    #[test]
    fn test_filter_std_functions_removes_outgoing_edges() {
        use crate::parser::{Call, Language, ParsedFile, Symbol, Visibility};
        use crate::types::LineRange;

        let mut file = ParsedFile::new(Language::Rust, PathBuf::from("test.rs"));
        file.symbols.push(Symbol::Function {
            name: "process".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(2, 5).unwrap(),
            is_async: false,
            attributes: vec![],
        });
        file.calls.push(Call {
            caller: Some("unwrap".to_string()),
            callee: "process".to_string(),
            line: 3,
            is_dynamic: false,
        });

        let mut graph = CallGraphBuilder::from_files(&[file]);
        assert!(graph.contains("unwrap"));

        let edges_before = graph.edge_count();
        graph.filter_std_functions();
        assert!(!graph.contains("unwrap"));
        assert!(graph.edge_count() < edges_before);
    }

    #[test]
    fn test_filter_std_functions_many_interleaved_nodes() {
        let mut graph = CallGraph::new();

        // Add interleaved std and user functions
        let std_names = [
            "unwrap", "clone", "collect", "parse", "len", "push", "pop", "iter", "map", "filter",
            "sort", "reverse",
        ];
        let user_names = [
            "process_data",
            "validate_input",
            "render_output",
            "compute_result",
            "transform_payload",
        ];

        // Add user functions with edges to std functions (interleaved)
        for (i, user) in user_names.iter().enumerate() {
            for (j, std_fn) in std_names.iter().enumerate() {
                graph.add_call(
                    user,
                    std_fn,
                    CallEdge {
                        line: i * 100 + j,
                        is_dynamic: false,
                    },
                );
            }
            // Also add edges between user functions
            if i + 1 < user_names.len() {
                graph.add_call(
                    user,
                    user_names[i + 1],
                    CallEdge {
                        line: i * 100 + 50,
                        is_dynamic: false,
                    },
                );
            }
        }

        let total_before = graph.function_count();
        assert_eq!(total_before, std_names.len() + user_names.len());

        graph.filter_std_functions();

        // Only user functions should remain
        assert_eq!(graph.function_count(), user_names.len());
        for user in &user_names {
            assert!(graph.contains(user), "User function {user} should remain");
        }
        for std_fn in &std_names {
            assert!(
                !graph.contains(std_fn),
                "Std function {std_fn} should be removed"
            );
        }

        // Verify graph consistency: name_to_index should match node count
        assert_eq!(graph.name_to_index.len(), graph.function_count());

        // Verify edges between user functions still work
        let callees = graph.callees("process_data");
        let callee_names: Vec<_> = callees.iter().map(|n| n.name.as_str()).collect();
        assert!(callee_names.contains(&"validate_input"));
    }

    #[test]
    fn test_deep_chain_no_false_cycle() {
        // BUG-059 regression: deep linear chain should not report cycle_detected
        let mut graph = CallGraph::new();

        // Build a linear chain: f0 -> f1 -> f2 -> ... -> f20
        for i in 0..20 {
            graph.add_call(
                &format!("f{i}"),
                &format!("f{}", i + 1),
                CallEdge {
                    line: i + 1,
                    is_dynamic: false,
                },
            );
        }

        // Trace with a depth limit smaller than the chain
        let result = graph.trace("f0", TraceDirection::Forward, 5).unwrap();
        assert!(
            !result.cycle_detected,
            "Deep linear chain should not report a cycle"
        );
    }

    // --- Tests targeting specific missed mutants ---

    #[test]
    fn test_is_std_function_returns_false_for_non_std() {
        // Catches: return false → return true on line 152
        assert!(!is_std_function("my_function"));
        assert!(!is_std_function("process"));
        assert!(!is_std_function("test_something")); // doesn't start with test_0_
    }

    #[test]
    fn test_is_std_function_test_0_prefix() {
        // Catches: starts_with("test_0_") → other string mutations on line 148
        assert!(is_std_function("test_0_abc"));
        assert!(!is_std_function("test_1_abc"));
        assert!(!is_std_function("test_abc"));
    }

    #[test]
    fn test_trace_empty_level_breaks_loop() {
        // Traces from a leaf node (no outgoing edges) should return empty levels
        // Catches: current_level.is_empty() break condition mutation (line 417)
        let mut graph = CallGraph::new();
        graph.add_function(FunctionNode::new("leaf"));
        let result = graph.trace("leaf", TraceDirection::Forward, 10).unwrap();
        assert!(result.levels.is_empty());
        assert!(!result.cycle_detected);
    }

    #[test]
    fn test_trace_cycle_detected() {
        // a -> b -> c -> a (cycle)
        // Catches: cycle_detected = true assignment (line 442) and visited.contains check (line 436)
        let mut graph = CallGraph::new();
        graph.add_call(
            "a",
            "b",
            CallEdge {
                line: 1,
                is_dynamic: false,
            },
        );
        graph.add_call(
            "b",
            "c",
            CallEdge {
                line: 2,
                is_dynamic: false,
            },
        );
        graph.add_call(
            "c",
            "a",
            CallEdge {
                line: 3,
                is_dynamic: false,
            },
        );

        let result = graph.trace("a", TraceDirection::Forward, 10).unwrap();
        assert!(result.cycle_detected);
    }

    #[test]
    fn test_trace_nonexistent_function_returns_none() {
        // Catches: name_to_index.get(start)? None path (line 405)
        let graph = CallGraph::new();
        assert!(graph
            .trace("nonexistent", TraceDirection::Forward, 5)
            .is_none());
    }

    #[test]
    fn test_format_tree_indent_depth() {
        // Catches: depth + 1 → depth * 1 mutation (line 271) and == → != (line 273)
        let mut graph = CallGraph::new();
        graph.add_call(
            "a",
            "b",
            CallEdge {
                line: 1,
                is_dynamic: false,
            },
        );

        let result = graph.trace("a", TraceDirection::Forward, 5).unwrap();
        let tree = result.format_tree();
        // At depth=0, indent should be "  " (2 spaces, repeat(0+1)=repeat(1))
        // If mutated to depth*1=depth=0, indent would be "" (empty)
        assert!(tree.contains("  "), "tree output should have indentation");
        // Check specific structure
        assert!(tree.contains("a calls:"));
        assert!(tree.contains("b"));
    }

    #[test]
    fn test_filter_std_removes_and_preserves_user() {
        // Catches: sort_by_key reverse order (line 536), removal from maps (lines 540-541)
        let mut graph = CallGraph::new();
        graph.add_call(
            "main",
            "unwrap",
            CallEdge {
                line: 1,
                is_dynamic: false,
            },
        );
        graph.add_call(
            "main",
            "process",
            CallEdge {
                line: 2,
                is_dynamic: false,
            },
        );

        graph.filter_std_functions();
        assert!(!graph.contains("unwrap"));
        assert!(graph.contains("main"));
        assert!(graph.contains("process"));
        // Index should be consistent
        assert_eq!(graph.name_to_index.len(), graph.function_count());
    }

    #[test]
    fn test_is_std_function_exact_match_only() {
        assert!(is_std_function("unwrap"));
        assert!(!is_std_function("unwrap_custom"));
        assert!(!is_std_function("my_unwrap"));
    }

    #[test]
    fn test_is_std_function_test_0_prefix_boundary() {
        assert!(is_std_function("test_0_"));
        assert!(!is_std_function("test_0"));
        assert!(!is_std_function("test_"));
    }

    #[test]
    fn test_add_function_existing_with_location_already_set() {
        let mut graph = CallGraph::new();
        graph.add_function(FunctionNode::with_location("f", PathBuf::from("a.rs"), 1));
        graph.add_function(FunctionNode::with_location("f", PathBuf::from("b.rs"), 2));
        let node = graph.get_node("f").unwrap();
        assert_eq!(node.file, Some(PathBuf::from("a.rs")));
        assert_eq!(node.line, Some(1));
        assert_eq!(graph.function_count(), 1);
    }

    #[test]
    fn test_trace_all_functions_dedup() {
        let mut graph = CallGraph::new();
        graph.add_call(
            "a",
            "b",
            CallEdge {
                line: 1,
                is_dynamic: false,
            },
        );
        graph.add_call(
            "a",
            "c",
            CallEdge {
                line: 2,
                is_dynamic: false,
            },
        );
        graph.add_call(
            "b",
            "c",
            CallEdge {
                line: 3,
                is_dynamic: false,
            },
        );

        let result = graph.trace("a", TraceDirection::Forward, 3).unwrap();
        let all = result.all_functions();
        let names: Vec<_> = all.iter().map(|n| n.name.as_str()).collect();
        let unique_count = names.len();
        let deduped: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(unique_count, deduped.len());
    }

    #[test]
    fn test_format_list_empty_levels_skipped() {
        let mut graph = CallGraph::new();
        graph.add_function(FunctionNode::new("isolated"));
        let result = graph.trace("isolated", TraceDirection::Forward, 5).unwrap();
        let list = result.format_list();
        assert!(list.contains("Called:"));
        assert!(!list.contains("[Depth"));
    }

    #[test]
    fn test_format_list_cycle_message() {
        let mut graph = CallGraph::new();
        graph.add_call(
            "x",
            "y",
            CallEdge {
                line: 1,
                is_dynamic: false,
            },
        );
        graph.add_call(
            "y",
            "x",
            CallEdge {
                line: 2,
                is_dynamic: false,
            },
        );
        let result = graph.trace("x", TraceDirection::Forward, 10).unwrap();
        assert!(result.cycle_detected);
        let list = result.format_list();
        assert!(list.contains("cycle detected"));
    }

    #[test]
    fn test_filter_std_on_empty_graph() {
        let mut graph = CallGraph::new();
        graph.filter_std_functions();
        assert_eq!(graph.function_count(), 0);
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn test_filter_std_rebuild_index_consistency() {
        let mut graph = CallGraph::new();
        graph.add_call(
            "collect",
            "user_fn",
            CallEdge {
                line: 1,
                is_dynamic: false,
            },
        );
        graph.add_call(
            "user_fn",
            "iter",
            CallEdge {
                line: 2,
                is_dynamic: false,
            },
        );
        graph.add_call(
            "user_fn",
            "process",
            CallEdge {
                line: 3,
                is_dynamic: false,
            },
        );

        graph.filter_std_functions();

        assert!(graph.contains("user_fn"));
        assert!(graph.contains("process"));
        assert!(!graph.contains("collect"));
        assert!(!graph.contains("iter"));

        assert_eq!(graph.name_to_index.len(), graph.function_count());
        for idx in graph.graph.node_indices() {
            let node = graph.graph.node_weight(idx).unwrap();
            assert_eq!(*graph.name_to_index.get(&node.name).unwrap(), idx);
        }
    }

    #[test]
    fn test_call_graph_builder_from_files_no_caller() {
        use crate::parser::{Call, Language, ParsedFile};

        let mut file = ParsedFile::new(Language::Rust, PathBuf::from("ext.rs"));
        file.calls.push(Call {
            caller: None,
            callee: "ext_lib_call".to_string(),
            line: 5,
            is_dynamic: true,
        });

        let graph = CallGraphBuilder::from_files(&[file]);
        assert!(graph.contains("ext_lib_call"));
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn test_trace_backward_from_root_empty_levels() {
        let graph = create_test_call_graph();
        let result = graph.trace("main", TraceDirection::Backward, 5).unwrap();
        assert!(result.levels.is_empty());
    }
}
