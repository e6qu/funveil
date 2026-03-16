use std::collections::HashSet;

use crate::error::Result;
use crate::parser::{ParsedFile, Symbol};
use crate::strategies::{HeaderStrategy, VeilStrategy};

pub enum LevelResult {
    Remove,
    Headers(String),
    HeadersAndCalled(String),
    FullSource,
}

pub fn apply_level(level: u8, content: &str, parsed: &ParsedFile) -> Result<LevelResult> {
    match level {
        0 => Ok(LevelResult::Remove),
        1 => {
            let strategy = HeaderStrategy::new();
            let veiled = strategy.veil_file(content, parsed)?;
            Ok(LevelResult::Headers(veiled))
        }
        2 => {
            let veiled = compute_level2(content, parsed);
            Ok(LevelResult::HeadersAndCalled(veiled))
        }
        3 => Ok(LevelResult::FullSource),
        _ => unreachable!("level must be 0..=3"),
    }
}

fn compute_level2(content: &str, parsed: &ParsedFile) -> String {
    let called_names: HashSet<String> = parsed.calls.iter().map(|c| c.callee.clone()).collect();

    let lines: Vec<&str> = content.lines().collect();
    let mut included_ranges: Vec<(usize, usize)> = Vec::new();

    for symbol in &parsed.symbols {
        match symbol {
            Symbol::Function {
                name,
                line_range,
                body_range,
                ..
            } => {
                if called_names.contains(name.as_str()) {
                    included_ranges.push((line_range.start() - 1, line_range.end() - 1));
                } else {
                    let sig_end = (body_range.start() - 1).min(line_range.end() - 1);
                    included_ranges.push((line_range.start() - 1, sig_end));
                }
            }
            Symbol::Class {
                methods,
                line_range,
                ..
            } => {
                included_ranges.push((line_range.start() - 1, line_range.start() - 1));
                for method in methods {
                    if let Symbol::Function {
                        name,
                        line_range: m_range,
                        body_range: m_body,
                        ..
                    } = method
                    {
                        if called_names.contains(name.as_str()) {
                            included_ranges.push((m_range.start() - 1, m_range.end() - 1));
                        } else {
                            let sig_end = (m_body.start() - 1).min(m_range.end() - 1);
                            included_ranges.push((m_range.start() - 1, sig_end));
                        }
                    }
                }
            }
            Symbol::Module { line_range, .. } => {
                included_ranges.push((line_range.start() - 1, line_range.start() - 1));
            }
        }
    }

    included_ranges.sort_by_key(|r| r.0);

    let mut output_lines: Vec<String> = Vec::new();
    let mut last_end: Option<usize> = None;
    for (start, end) in &included_ranges {
        if let Some(le) = last_end {
            if *start > le + 1 {
                output_lines.push("// ...".to_string());
            }
        }
        for line in lines
            .iter()
            .take((*end).min(lines.len().saturating_sub(1)) + 1)
            .skip(*start)
        {
            output_lines.push(line.to_string());
        }
        last_end = Some(*end);
    }

    output_lines.join("\n") + "\n"
}

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::TreeSitterParser;
    use std::path::Path;

    fn parse_rust(code: &str) -> ParsedFile {
        let parser = TreeSitterParser::new().expect("parser init failed");
        parser
            .parse_file(Path::new("test.rs"), code)
            .expect("parse failed")
    }

    #[test]
    fn level0_returns_remove() {
        let parsed = parse_rust("fn main() {}");
        let result = apply_level(0, "fn main() {}", &parsed).unwrap();
        assert!(matches!(result, LevelResult::Remove));
    }

    #[test]
    fn level1_returns_headers_with_signature() {
        let code = "fn greet(name: &str) -> String {\n    format!(\"hi {name}\")\n}\n";
        let parsed = parse_rust(code);
        let result = apply_level(1, code, &parsed).unwrap();
        match result {
            LevelResult::Headers(veiled) => {
                assert!(veiled.contains("fn greet"));
                assert!(!veiled.contains("format!"));
            }
            _ => panic!("expected LevelResult::Headers"),
        }
    }

    #[test]
    fn level2_includes_called_bodies_and_omits_uncalled() {
        let code = concat!(
            "fn caller() {\n",
            "    helper();\n",
            "}\n",
            "\n",
            "fn helper() {\n",
            "    do_work();\n",
            "}\n",
            "\n",
            "fn unused() {\n",
            "    secret();\n",
            "}\n",
        );
        let parsed = parse_rust(code);
        let result = apply_level(2, code, &parsed).unwrap();
        match result {
            LevelResult::HeadersAndCalled(veiled) => {
                assert!(
                    veiled.contains("do_work"),
                    "called function 'helper' body should be fully included"
                );
                assert!(
                    !veiled.contains("secret"),
                    "uncalled function 'unused' body should be omitted"
                );
                assert!(veiled.contains("fn unused"));
            }
            _ => panic!("expected LevelResult::HeadersAndCalled"),
        }
    }

    #[test]
    fn level3_returns_full_source() {
        let parsed = parse_rust("fn main() {}");
        let result = apply_level(3, "fn main() {}", &parsed).unwrap();
        assert!(matches!(result, LevelResult::FullSource));
    }
}
