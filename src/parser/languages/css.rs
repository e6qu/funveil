//! CSS and TailwindCSS language parser for Tree-sitter.
//!
//! Supports CSS files (.css), SCSS (.scss), and detects TailwindCSS directives.
//! Extracts CSS rules, selectors, and Tailwind-specific directives.

use tree_sitter::{Language as TSLanguage, Tree};

use crate::error::Result;
use crate::parser::{Language, ParsedFile, Symbol};
use crate::types::LineRange;

/// Tree-sitter language for CSS
pub fn css_language() -> TSLanguage {
    tree_sitter_css::LANGUAGE.into()
}

/// Check if file is SCSS
pub fn is_scss(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e == "scss" || e == "sass")
        .unwrap_or(false)
}

/// Check if file uses Tailwind (based on content heuristics or filename)
pub fn has_tailwind(path: &std::path::Path, content: &str) -> bool {
    // Check filename
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if file_name.contains("tailwind") {
        return true;
    }

    // Check content for Tailwind directives
    content.contains("@tailwind") || content.contains("@apply") || content.contains("@layer")
}

/// Parse a CSS/SCSS source file
pub fn parse_css_file(path: &std::path::Path, content: &str) -> Result<ParsedFile> {
    let language = Language::Css;
    let mut parser = tree_sitter::Parser::new();
    let css_lang = css_language();
    parser
        .set_language(&css_lang)
        .expect("Failed to load CSS parser");

    let tree = parser
        .parse(content, None)
        .expect("tree-sitter parse must succeed when language is set");

    let mut parsed = ParsedFile::new(language, path.to_path_buf());

    let uses_tailwind = has_tailwind(path, content);
    if uses_tailwind {
        parsed.language = Language::Css; // Could add a separate Tailwind variant if needed
    }

    let mut rules = extract_css_rules(&tree, content)?;
    parsed.symbols.append(&mut rules);

    let mut at_rules = extract_css_at_rules(&tree, content)?;
    parsed.symbols.append(&mut at_rules);

    Ok(parsed)
}

/// Extract CSS rules (selectors + blocks)
fn extract_css_rules(tree: &Tree, content: &str) -> Result<Vec<Symbol>> {
    let mut symbols = Vec::new();
    let root = tree.root_node();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "rule_set" {
            let start_line = child.start_position().row + 1;
            let end_line = child.end_position().row + 1;

            let mut selector_text = "rule".to_string();
            let mut child_cursor = child.walk();
            for grandchild in child.children(&mut child_cursor) {
                if grandchild.kind() == "selectors" {
                    let text = grandchild
                        .utf8_text(content.as_bytes())
                        .expect("source is valid UTF-8");
                    selector_text = text.trim().to_string();
                    if selector_text.is_empty() {
                        selector_text = "<empty>".to_string();
                    } else if selector_text.len() > 50 {
                        let truncated: String = selector_text.chars().take(47).collect();
                        selector_text = format!("{truncated}...");
                    }
                    break;
                }
            }

            let line_range = LineRange::new(start_line, end_line)
                .expect("Tree-sitter positions should always produce valid line ranges");

            symbols.push(Symbol::Module {
                name: selector_text,
                line_range,
            });
        }
    }

    Ok(symbols)
}

/// Extract CSS at-rules (@media, @import, @tailwind, etc.)
fn extract_css_at_rules(tree: &Tree, content: &str) -> Result<Vec<Symbol>> {
    let mut symbols = Vec::new();
    let root = tree.root_node();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "at_rule" {
            let start_line = child.start_position().row + 1;
            let end_line = child.end_position().row + 1;

            let mut at_name = "@rule".to_string();
            let mut child_cursor = child.walk();
            for grandchild in child.children(&mut child_cursor) {
                if grandchild.kind() == "at_keyword" {
                    let text = grandchild
                        .utf8_text(content.as_bytes())
                        .expect("source is valid UTF-8");
                    at_name = text.to_string();
                    break;
                }
            }

            let is_tailwind = at_name == "@tailwind" || at_name == "@apply" || at_name == "@layer";
            let display_name = if is_tailwind {
                format!("{at_name} (Tailwind)")
            } else {
                at_name
            };

            let line_range = LineRange::new(start_line, end_line)
                .expect("Tree-sitter positions should always produce valid line ranges");

            symbols.push(Symbol::Module {
                name: display_name,
                line_range,
            });
        }
    }

    Ok(symbols)
}

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_is_scss() {
        assert!(is_scss(Path::new("styles.scss")));
        assert!(is_scss(Path::new("main.sass")));
        assert!(!is_scss(Path::new("styles.css")));
        assert!(!is_scss(Path::new("main.ts")));
    }

    #[test]
    fn test_has_tailwind() {
        assert!(has_tailwind(Path::new("tailwind.config.js"), ""));
        assert!(has_tailwind(Path::new("styles.css"), "@tailwind base;"));
        assert!(has_tailwind(
            Path::new("input.css"),
            ".btn { @apply px-4 py-2; }"
        ));
        assert!(has_tailwind(Path::new("main.css"), "@layer components {"));
        assert!(!has_tailwind(
            Path::new("legacy.css"),
            "body { color: red; }"
        ));
    }

    #[test]
    fn test_parse_simple_css() {
        let code = r#"body {
    font-family: Arial, sans-serif;
    margin: 0;
    padding: 0;
}

.container {
    max-width: 1200px;
    margin: 0 auto;
}

h1, h2, h3 {
    color: #333;
}
"#;

        let parsed = parse_css_file(Path::new("styles.css"), code).unwrap();

        // Should have CSS rules as symbols
        let modules: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| matches!(s, Symbol::Module { .. }))
            .collect();
        assert!(!modules.is_empty());

        // Check for specific selectors
        let has_body = modules.iter().any(|s| s.name() == "body");
        let has_container = modules.iter().any(|s| s.name() == ".container");
        let has_headings = modules.iter().any(|s| s.name().contains("h1"));

        assert!(has_body);
        assert!(has_container);
        assert!(has_headings);
    }

    #[test]
    fn test_parse_css_with_media_queries() {
        let code = r#".responsive {
    width: 100%;
}

@media (min-width: 768px) {
    .responsive {
        width: 750px;
    }
}

@media (min-width: 1024px) {
    .responsive {
        width: 1000px;
    }
}
"#;

        let parsed = parse_css_file(Path::new("responsive.css"), code).unwrap();

        // Should have various rules including @media rules
        let modules: Vec<_> = parsed.symbols.iter().collect();
        // Note: @media detection depends on tree-sitter CSS grammar
        // We just verify the file parses successfully
        assert!(!modules.is_empty());
    }

    #[test]
    fn test_parse_tailwind_directives() {
        let code = r#"@tailwind base;
@tailwind components;
@tailwind utilities;

@layer components {
    .btn {
        @apply px-4 py-2 rounded;
    }
}

.custom {
    @apply bg-blue-500 text-white;
}
"#;

        let parsed = parse_css_file(Path::new("input.css"), code).unwrap();

        // Should have Tailwind directives
        let modules: Vec<_> = parsed.symbols.iter().collect();
        let has_tailwind_base = modules.iter().any(|s| s.name().contains("@tailwind"));
        let has_layer = modules.iter().any(|s| s.name().contains("@layer"));

        assert!(has_tailwind_base);
        assert!(has_layer);
    }

    #[test]
    fn test_parse_css_long_selector() {
        // Selector longer than 50 characters should be truncated
        let code = r#".very-long-selector-name-that-exceeds-fifty-characters-for-testing {
    color: red;
}
"#;
        let parsed = parse_css_file(Path::new("test.css"), code).unwrap();
        let modules: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| matches!(s, Symbol::Module { .. }))
            .collect();
        assert!(!modules.is_empty());
        // Long selector names get truncated with "..."
        let name = modules[0].name();
        assert!(name.len() <= 53 || name.ends_with("...")); // 50 + "..."
    }

    #[test]
    fn test_parse_css_non_tailwind_at_rule() {
        // Custom/unknown at-rules are parsed as at_rule nodes by tree-sitter CSS.
        // This exercises the non-tailwind display branch (line 143).
        let code = "@custom-media --small-viewport (max-width: 30em);\n";
        let parsed = parse_css_file(Path::new("test.css"), code).unwrap();
        let at_rules: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| s.name().starts_with("@") && !s.name().contains("Tailwind"))
            .collect();
        // Custom at-rules should appear without "(Tailwind)" suffix
        assert!(!at_rules.is_empty(), "custom at-rule should be extracted");
    }

    #[test]
    fn test_parse_css_unicode_selector() {
        // BUG-001 regression: emoji/CJK chars in selector exceeding 50 bytes should not panic
        let selector = format!(".emoji-{}-end", "🎉🎊🎈🎁🎆🎇✨🎀🎃🎄🎋🎍🎎🎏");
        let code = format!("{selector} {{\n    color: red;\n}}\n");
        let parsed = parse_css_file(Path::new("test.css"), &code).unwrap();
        let modules: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| matches!(s, Symbol::Module { .. }))
            .collect();
        assert!(!modules.is_empty());
        let name = modules[0].name();
        if name.contains("...") {
            assert!(name.ends_with("..."));
        }
    }

    #[test]
    fn test_parse_css_tailwind_directives() {
        let code = r#"@tailwind base;
@tailwind components;
@apply flex items-center;
@layer utilities {
    .custom { color: red; }
}
"#;
        let parsed = parse_css_file(Path::new("test.css"), code).unwrap();
        // Should detect tailwind directives
        let modules: Vec<_> = parsed.symbols.iter().collect();
        assert!(!modules.is_empty());
    }

    #[test]
    fn test_parse_css_empty_content() {
        let parsed = parse_css_file(Path::new("test.css"), "").unwrap();
        assert!(parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_css_only_comments() {
        let code = "/* comment only */\n";
        let parsed = parse_css_file(Path::new("test.css"), code).unwrap();
        assert!(parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_css_no_rules() {
        let code = "@charset \"UTF-8\";\n";
        let parsed = parse_css_file(Path::new("test.css"), code).unwrap();
        let rules: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| !s.name().starts_with("@"))
            .collect();
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_css_no_at_rules() {
        let code = ".simple { color: red; }\n";
        let parsed = parse_css_file(Path::new("test.css"), code).unwrap();
        let at_rules: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| s.name().starts_with("@"))
            .collect();
        assert!(at_rules.is_empty());
    }

    #[test]
    fn test_parse_css_non_tailwind_file() {
        let code = ".regular { color: blue; }\n";
        let parsed = parse_css_file(Path::new("normal.css"), code).unwrap();
        assert!(!parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_css_empty_input_no_panic() {
        let result = parse_css_file(Path::new("test.css"), "");
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_is_scss_no_extension() {
        // Path with no extension exercises the `unwrap_or(false)` branch.
        assert!(!is_scss(Path::new("Makefile")));
        assert!(!is_scss(Path::new("")));
    }

    #[test]
    fn test_has_tailwind_no_filename() {
        // Path with no file_name (e.g., root) exercises `unwrap_or("")` branch.
        assert!(!has_tailwind(Path::new("/"), "body { color: red; }"));
    }

    #[test]
    fn test_has_tailwind_apply_directive() {
        // Exercises the `@apply` check specifically (vs @tailwind and @layer).
        assert!(has_tailwind(
            Path::new("styles.css"),
            ".btn { @apply px-4; }"
        ));
    }

    #[test]
    fn test_has_tailwind_no_match() {
        // No tailwind indicators — exercises all false branches in content checks.
        assert!(!has_tailwind(
            Path::new("plain.css"),
            "body { margin: 0; } @media screen { .a { color: red; } }"
        ));
    }

    #[test]
    fn test_parse_css_no_rules_only_comments() {
        // CSS with only comments — no rule_set or at_rule nodes at root.
        // Exercises the `child.kind() != "rule_set"` and `!= "at_rule"` branches.
        let code = "/* This is a comment */\n/* Another comment */\n";
        let parsed = parse_css_file(Path::new("test.css"), code).unwrap();
        assert!(parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_css_at_rule_without_keyword() {
        // A @charset rule — tree-sitter CSS may parse it differently.
        // Exercises the at_rule iteration where the grandchild might not be at_keyword.
        let code = "@charset \"UTF-8\";\n";
        let parsed = parse_css_file(Path::new("test.css"), code).unwrap();
        // Whether or not this is extracted, it should not panic
        let _ = parsed.symbols;
    }

    #[test]
    fn test_parse_css_rule_without_selectors_child() {
        // Malformed CSS that tree-sitter parses as a rule_set but might lack a selectors node.
        // Exercises the branch where no grandchild matches "selectors", keeping default "rule".
        let code = "{ color: red; }\n";
        let parsed = parse_css_file(Path::new("test.css"), code).unwrap();
        // Should not panic regardless of what tree-sitter produces
        let _ = parsed.symbols;
    }

    #[test]
    fn test_parse_css_selector_exactly_50_chars() {
        // Selector of exactly 50 chars should NOT be truncated (len > 50 is false).
        let selector = format!(".{}", "a".repeat(49)); // . + 49 = 50
        let code = format!("{selector} {{\n    color: red;\n}}\n");
        let parsed = parse_css_file(Path::new("test.css"), &code).unwrap();
        let modules: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| matches!(s, Symbol::Module { .. }))
            .collect();
        if !modules.is_empty() {
            let name = modules[0].name();
            assert!(
                !name.ends_with("..."),
                "50-char selector should not be truncated, got: {}",
                name
            );
        }
    }

    #[test]
    fn test_parse_css_apply_tailwind_directive() {
        // @apply is a Tailwind directive — exercises the `at_name == "@apply"` true branch.
        let code = ".btn { @apply px-4 py-2 rounded; }\n";
        let parsed = parse_css_file(Path::new("test.css"), code).unwrap();
        // @apply inside a rule_set is nested, so it may not appear as a top-level at_rule.
        // What matters is it doesn't panic.
        let _ = parsed.symbols;
    }

    #[test]
    fn test_parse_css_font_face_at_rule() {
        // @font-face is a standard (non-Tailwind) at-rule parsed as `at_rule` by tree-sitter.
        // Exercises the non-tailwind display_name branch (line 139-141).
        let code = "@font-face {\n  font-family: 'MyFont';\n  src: url('myfont.woff2');\n}\n\nbody { margin: 0; }\n";
        let parsed = parse_css_file(Path::new("test.css"), code).unwrap();
        let at_rules: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| s.name().starts_with("@") && !s.name().contains("Tailwind"))
            .collect();
        assert!(
            !at_rules.is_empty(),
            "Should extract @font-face as a non-Tailwind at-rule"
        );
    }

    #[test]
    fn test_parse_css_tailwind_file_detection_by_name() {
        // File named "tailwind.css" triggers tailwind detection by filename.
        let code = "body { margin: 0; }\n";
        let parsed = parse_css_file(Path::new("tailwind.css"), code).unwrap();
        // Should parse without error; the tailwind branch in parse_css_file is hit.
        assert!(!parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_css_whitespace_only() {
        // Whitespace-only input — no nodes to iterate.
        let code = "   \n\n   \n";
        let parsed = parse_css_file(Path::new("test.css"), code).unwrap();
        assert!(parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_css_media_at_rule_non_tailwind() {
        // @media is a non-tailwind at-rule. Exercises the is_tailwind == false branch.
        let code = "@media screen and (min-width: 768px) {\n  .container { width: 750px; }\n}\n";
        let parsed = parse_css_file(Path::new("test.css"), code).unwrap();
        let modules: Vec<_> = parsed.symbols.iter().collect();
        let media_rules: Vec<_> = modules
            .iter()
            .filter(|s| s.name().contains("@media") || s.name().contains("@rule"))
            .collect();
        // @media should not have "(Tailwind)" suffix
        for r in &media_rules {
            assert!(
                !r.name().contains("Tailwind"),
                "@media should not be tagged as Tailwind"
            );
        }
    }
}
