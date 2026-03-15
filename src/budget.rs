use crate::analysis::CallGraph;
use crate::config::Config;
use crate::error::Result;
use crate::metadata::MetadataIndex;
use std::path::Path;

/// Estimate token count from character count (~4 chars per token)
pub fn estimate_tokens(content: &str) -> usize {
    content.len() / 4
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DisclosurePlan {
    pub budget: usize,
    pub used_tokens: usize,
    pub focus: String,
    pub entries: Vec<DisclosureEntry>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DisclosureEntry {
    pub file: String,
    pub level: u8,
    pub estimated_tokens: usize,
}

/// Compute a disclosure plan given a token budget and focus path.
///
/// Strategy (greedy):
/// 1. Level 3 (full source) for the focus file
/// 2. Level 2 (signatures + called bodies) for direct dependencies
/// 3. Level 1 (signatures only) for remaining reachable code
/// 4. Stop when budget exhausted
pub fn compute_disclosure_plan(
    root: &Path,
    config: &Config,
    budget: usize,
    focus: &str,
    graph: Option<&CallGraph>,
    index: Option<&MetadataIndex>,
) -> Result<DisclosurePlan> {
    let cas = crate::cas::ContentStore::new(root);
    let mut entries = Vec::new();
    let mut used = 0usize;

    if let Some(meta) = config.get_object(focus) {
        if let Ok(hash) = crate::types::ContentHash::from_string(meta.hash.clone()) {
            if let Ok(content) = cas.retrieve(&hash) {
                let tokens = estimate_tokens(&String::from_utf8_lossy(&content));
                if used + tokens <= budget {
                    entries.push(DisclosureEntry {
                        file: focus.to_string(),
                        level: 3,
                        estimated_tokens: tokens,
                    });
                    used += tokens;
                }
            }
        }
    }

    let mut direct_deps = Vec::new();
    let mut reachable = Vec::new();

    if let (Some(graph), Some(index)) = (graph, index) {
        let focus_funcs: Vec<String> = index
            .symbols
            .iter()
            .filter(|(_, entries)| entries.iter().any(|e| e.file == focus))
            .map(|(name, _)| name.clone())
            .collect();

        let mut seen_files = std::collections::HashSet::new();
        seen_files.insert(focus.to_string());

        for func in &focus_funcs {
            let callees = graph.callees(func);
            for callee in callees {
                if let Some(ref file) = callee.file {
                    let file_str = file.to_string_lossy().to_string();
                    if !seen_files.contains(&file_str) {
                        seen_files.insert(file_str.clone());
                        direct_deps.push(file_str);
                    }
                }
            }
        }

        for func in &focus_funcs {
            if let Some(trace) = graph.trace(func, crate::analysis::TraceDirection::Forward, 5) {
                for node in trace.all_functions() {
                    if let Some(ref file) = node.file {
                        let file_str = file.to_string_lossy().to_string();
                        if !seen_files.contains(&file_str) {
                            seen_files.insert(file_str.clone());
                            reachable.push(file_str);
                        }
                    }
                }
            }
        }
    }

    for file in &direct_deps {
        if let Some(meta) = config.get_object(file) {
            if let Ok(hash) = crate::types::ContentHash::from_string(meta.hash.clone()) {
                if let Ok(content) = cas.retrieve(&hash) {
                    let full_tokens = estimate_tokens(&String::from_utf8_lossy(&content));
                    let tokens = full_tokens * 60 / 100;
                    if used + tokens <= budget {
                        entries.push(DisclosureEntry {
                            file: file.clone(),
                            level: 2,
                            estimated_tokens: tokens,
                        });
                        used += tokens;
                    }
                }
            }
        }
    }

    for file in &reachable {
        if let Some(meta) = config.get_object(file) {
            if let Ok(hash) = crate::types::ContentHash::from_string(meta.hash.clone()) {
                if let Ok(content) = cas.retrieve(&hash) {
                    let full_tokens = estimate_tokens(&String::from_utf8_lossy(&content));
                    let tokens = full_tokens * 20 / 100;
                    if used + tokens <= budget {
                        entries.push(DisclosureEntry {
                            file: file.clone(),
                            level: 1,
                            estimated_tokens: tokens,
                        });
                        used += tokens;
                    }
                }
            }
        }
    }

    Ok(DisclosurePlan {
        budget,
        used_tokens: used,
        focus: focus.to_string(),
        entries,
    })
}

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcdefgh"), 2);
        assert_eq!(estimate_tokens(&"a".repeat(100)), 25);
    }

    #[test]
    fn test_empty_disclosure_plan() {
        let temp = tempfile::TempDir::new().unwrap();
        crate::config::ensure_data_dir(temp.path()).unwrap();
        let config = Config::new(crate::types::Mode::Whitelist);
        let plan =
            compute_disclosure_plan(temp.path(), &config, 1000, "nonexistent.rs", None, None)
                .unwrap();
        assert_eq!(plan.used_tokens, 0);
        assert!(plan.entries.is_empty());
    }

    #[test]
    fn test_disclosure_plan_with_focus() {
        let temp = tempfile::TempDir::new().unwrap();
        crate::config::ensure_data_dir(temp.path()).unwrap();
        let mut config = Config::new(crate::types::Mode::Whitelist);
        let store = crate::cas::ContentStore::new(temp.path());

        let content = "fn main() { println!(\"hello\"); }\n";
        let hash = store.store(content.as_bytes()).unwrap();
        config.register_object(
            "main.rs".to_string(),
            crate::config::ObjectMeta::new(hash, 0o644),
        );

        let plan =
            compute_disclosure_plan(temp.path(), &config, 1000, "main.rs", None, None).unwrap();
        assert_eq!(plan.entries.len(), 1);
        assert_eq!(plan.entries[0].file, "main.rs");
        assert_eq!(plan.entries[0].level, 3);
        assert!(plan.used_tokens > 0);
        assert!(plan.used_tokens <= plan.budget);
    }

    #[test]
    fn test_disclosure_plan_budget_exhausted() {
        let temp = tempfile::TempDir::new().unwrap();
        crate::config::ensure_data_dir(temp.path()).unwrap();
        let mut config = Config::new(crate::types::Mode::Whitelist);
        let store = crate::cas::ContentStore::new(temp.path());

        let content = "fn main() { println!(\"hello world\"); }\n";
        let hash = store.store(content.as_bytes()).unwrap();
        config.register_object(
            "main.rs".to_string(),
            crate::config::ObjectMeta::new(hash, 0o644),
        );

        let plan = compute_disclosure_plan(temp.path(), &config, 1, "main.rs", None, None).unwrap();
        assert!(plan.entries.is_empty());
        assert_eq!(plan.used_tokens, 0);
    }

    #[test]
    fn test_disclosure_plan_with_graph_and_index() {
        let temp = tempfile::TempDir::new().unwrap();
        crate::config::ensure_data_dir(temp.path()).unwrap();
        let mut config = Config::new(crate::types::Mode::Whitelist);
        let store = crate::cas::ContentStore::new(temp.path());
        let meta_store = crate::metadata::MetadataStore::new(temp.path());

        let focus_content = "fn focus_fn() {\n    dep_fn();\n}\n";
        let focus_hash = store.store(focus_content.as_bytes()).unwrap();
        meta_store
            .store_metadata(&focus_hash, "focus.rs", focus_content)
            .unwrap();
        config.register_object(
            "focus.rs".to_string(),
            crate::config::ObjectMeta::new(focus_hash, 0o644),
        );

        let dep_content = "fn dep_fn() {\n    println!(\"dep\");\n}\n";
        let dep_hash = store.store(dep_content.as_bytes()).unwrap();
        meta_store
            .store_metadata(&dep_hash, "dep.rs", dep_content)
            .unwrap();
        config.register_object(
            "dep.rs".to_string(),
            crate::config::ObjectMeta::new(dep_hash, 0o644),
        );

        let index = crate::metadata::rebuild_index(temp.path(), &config).unwrap();
        let graph = crate::metadata::build_call_graph_from_metadata(temp.path(), &config).unwrap();

        let plan = compute_disclosure_plan(
            temp.path(),
            &config,
            10000,
            "focus.rs",
            Some(&graph),
            Some(&index),
        )
        .unwrap();

        assert!(!plan.entries.is_empty());
        assert_eq!(plan.entries[0].file, "focus.rs");
        assert_eq!(plan.entries[0].level, 3);
        assert!(plan.used_tokens > 0);
        assert!(plan.used_tokens <= plan.budget);
    }
}
