//! Fuzz the tree-sitter parser with arbitrary source code across all languages.
//!
//! Target: TreeSitterParser::parse_file

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::path::PathBuf;

fuzz_target!(|data: &[u8]| {
    if let Ok(content) = std::str::from_utf8(data) {
        // Limit input size to avoid timeouts on pathological inputs
        if content.len() > 10_000 {
            return;
        }

        let parser = match funveil::parser::TreeSitterParser::new() {
            Ok(p) => p,
            Err(_) => return,
        };

        // Try parsing as each supported language
        let extensions = [
            "rs", "ts", "tsx", "py", "sh", "go", "zig", "html", "css", "xml", "md", "tf",
            "yaml", "yml",
        ];

        for ext in &extensions {
            let path = PathBuf::from(format!("fuzz_input.{ext}"));
            // Should never panic regardless of content
            let _ = parser.parse_file(&path, content);
        }
    }
});
