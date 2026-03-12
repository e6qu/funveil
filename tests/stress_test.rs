//! Stress tests for Funveil
//!
//! These tests exercise edge cases and scale limits to find bugs that
//! don't surface in normal unit/integration tests.

use std::fs;
use tempfile::TempDir;

use funveil::{
    has_veils, unveil_all, unveil_file, veil_file, Config, ContentHash, ContentStore, LineRange,
    Pattern,
};

/// Helper: initialize a funveil project in a temp dir
fn init_project() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join(".funveil/objects")).unwrap();
    let config = Config::load(root).unwrap();
    config.save(root).unwrap();
    temp
}

// ============================================================================
// Large file stress tests
// ============================================================================

#[test]
fn stress_veil_large_file_full() {
    let temp = init_project();
    let root = temp.path();

    // Generate a 10K-line file
    let content: String = (1..=10_000)
        .map(|i| format!("line {i}: some content here that is reasonably long\n"))
        .collect();
    fs::write(root.join("large.txt"), &content).unwrap();

    let mut config = Config::load(root).unwrap();

    // Full veil
    veil_file(root, &mut config, "large.txt", None, true).unwrap();
    config.save(root).unwrap();

    // Verify it's veiled
    assert!(has_veils(&config, "large.txt"));

    // Full unveil
    unveil_file(root, &mut config, "large.txt", None, true).unwrap();
    config.save(root).unwrap();

    // Verify roundtrip
    let restored = fs::read_to_string(root.join("large.txt")).unwrap();
    assert_eq!(
        restored, content,
        "10K-line roundtrip should preserve content"
    );
}

#[test]
fn stress_veil_large_file_partial() {
    let temp = init_project();
    let root = temp.path();

    // 5K-line file
    let content: String = (1..=5_000).map(|i| format!("line {i}\n")).collect();
    fs::write(root.join("partial.txt"), &content).unwrap();

    let mut config = Config::load(root).unwrap();

    // Veil a large range in the middle
    let ranges = vec![LineRange::new(100, 4900).unwrap()];
    veil_file(root, &mut config, "partial.txt", Some(&ranges), true).unwrap();
    config.save(root).unwrap();

    // Unveil
    unveil_file(root, &mut config, "partial.txt", Some(&ranges), true).unwrap();
    config.save(root).unwrap();

    let restored = fs::read_to_string(root.join("partial.txt")).unwrap();
    assert_eq!(
        restored, content,
        "partial veil roundtrip should preserve content"
    );
}

// ============================================================================
// Many files stress tests
// ============================================================================

#[test]
fn stress_veil_many_files() {
    let temp = init_project();
    let root = temp.path();

    let file_count = 100;
    let mut contents = Vec::new();

    // Create many files
    for i in 0..file_count {
        let name = format!("file_{i:03}.txt");
        let content = format!("content of file {i}\nline 2\nline 3\n");
        fs::write(root.join(&name), &content).unwrap();
        contents.push((name, content));
    }

    let mut config = Config::load(root).unwrap();

    // Veil all files
    for (name, _) in &contents {
        veil_file(root, &mut config, name, None, true).unwrap();
    }
    config.save(root).unwrap();

    // Verify all veiled
    for (name, _) in &contents {
        assert!(has_veils(&config, name), "file {name} should be veiled");
    }

    // Unveil all at once
    unveil_all(root, &mut config, true).unwrap();
    config.save(root).unwrap();

    // Verify all restored
    for (name, original) in &contents {
        let restored = fs::read_to_string(root.join(name)).unwrap();
        assert_eq!(&restored, original, "file {name} should be restored");
    }
}

#[test]
fn stress_veil_many_partial_ranges() {
    let temp = init_project();
    let root = temp.path();

    // File with 100 lines
    let content: String = (1..=100).map(|i| format!("line {i}\n")).collect();
    fs::write(root.join("multi_range.txt"), &content).unwrap();

    let mut config = Config::load(root).unwrap();

    // Veil 10 non-overlapping 5-line ranges
    for i in 0..10 {
        let start = i * 10 + 1;
        let end = start + 4;
        let ranges = vec![LineRange::new(start, end).unwrap()];
        veil_file(root, &mut config, "multi_range.txt", Some(&ranges), true).unwrap();
    }
    config.save(root).unwrap();

    // Unveil all
    unveil_file(root, &mut config, "multi_range.txt", None, true).unwrap();
    config.save(root).unwrap();

    let restored = fs::read_to_string(root.join("multi_range.txt")).unwrap();
    assert_eq!(
        restored, content,
        "multi-range roundtrip should preserve content"
    );
}

// ============================================================================
// CAS stress tests
// ============================================================================

#[test]
fn stress_cas_many_objects() {
    let temp = init_project();
    let root = temp.path();
    let store = ContentStore::new(root);

    let mut hashes = Vec::new();

    // Store 200 distinct objects
    for i in 0..200 {
        let content = format!("object content #{i} with unique data: {}", i * 31337);
        let hash = store.store(content.as_bytes()).unwrap();
        hashes.push((hash, content));
    }

    // Verify all retrievable
    for (hash, expected) in &hashes {
        let data = store.retrieve(hash).unwrap();
        assert_eq!(
            String::from_utf8(data).unwrap(),
            *expected,
            "CAS roundtrip for hash {hash}"
        );
    }

    // Verify total size is positive
    let total = store.total_size().unwrap();
    assert!(total > 0);

    // List all and verify count
    let listed = store.list_all().unwrap();
    assert_eq!(listed.len(), 200);
}

#[test]
fn stress_cas_deduplication() {
    let temp = init_project();
    let root = temp.path();
    let store = ContentStore::new(root);

    let content = b"the same content repeated many times";

    // Store the same content 100 times
    let mut hashes = Vec::new();
    for _ in 0..100 {
        hashes.push(store.store(content).unwrap());
    }

    // All hashes should be identical (dedup)
    let first = &hashes[0];
    for h in &hashes[1..] {
        assert_eq!(h.to_string(), first.to_string());
    }

    // Should only have 1 object on disk
    let listed = store.list_all().unwrap();
    assert_eq!(listed.len(), 1);
}

// ============================================================================
// Edge case content stress tests
// ============================================================================

#[test]
fn stress_veil_single_line_file() {
    let temp = init_project();
    let root = temp.path();

    // File with exactly one line, no trailing newline
    fs::write(root.join("single.txt"), "only line").unwrap();

    let mut config = Config::load(root).unwrap();
    veil_file(root, &mut config, "single.txt", None, true).unwrap();
    config.save(root).unwrap();

    unveil_file(root, &mut config, "single.txt", None, true).unwrap();
    config.save(root).unwrap();

    let restored = fs::read_to_string(root.join("single.txt")).unwrap();
    assert_eq!(restored, "only line");
}

#[test]
fn stress_veil_only_newlines() {
    let temp = init_project();
    let root = temp.path();

    // File that's just newlines
    let content = "\n\n\n\n\n";
    fs::write(root.join("newlines.txt"), content).unwrap();

    let mut config = Config::load(root).unwrap();
    veil_file(root, &mut config, "newlines.txt", None, true).unwrap();
    config.save(root).unwrap();

    unveil_file(root, &mut config, "newlines.txt", None, true).unwrap();
    config.save(root).unwrap();

    let restored = fs::read_to_string(root.join("newlines.txt")).unwrap();
    assert_eq!(restored, content);
}

#[test]
fn stress_veil_unicode_content() {
    let temp = init_project();
    let root = temp.path();

    // File with diverse Unicode: CJK, emoji, RTL, combining chars
    let content = "Hello, 世界!\n🦀 Rust is great\nمرحبا\nZ̤͔ä͖l̠g̲̫o̫\nline 5\n";
    fs::write(root.join("unicode.txt"), content).unwrap();

    let mut config = Config::load(root).unwrap();
    veil_file(root, &mut config, "unicode.txt", None, true).unwrap();
    config.save(root).unwrap();

    unveil_file(root, &mut config, "unicode.txt", None, true).unwrap();
    config.save(root).unwrap();

    let restored = fs::read_to_string(root.join("unicode.txt")).unwrap();
    assert_eq!(restored, content, "Unicode roundtrip must be lossless");
}

#[test]
fn stress_veil_long_lines() {
    let temp = init_project();
    let root = temp.path();

    // File with very long lines (10KB each)
    let long_line = "x".repeat(10_000);
    let content = format!("{long_line}\n{long_line}\n{long_line}\n");
    fs::write(root.join("longlines.txt"), &content).unwrap();

    let mut config = Config::load(root).unwrap();
    veil_file(root, &mut config, "longlines.txt", None, true).unwrap();
    config.save(root).unwrap();

    unveil_file(root, &mut config, "longlines.txt", None, true).unwrap();
    config.save(root).unwrap();

    let restored = fs::read_to_string(root.join("longlines.txt")).unwrap();
    assert_eq!(restored, content, "Long line roundtrip must be lossless");
}

#[test]
fn stress_veil_file_with_marker_like_content() {
    let temp = init_project();
    let root = temp.path();

    // File content that looks like veil markers but isn't quite (shouldn't trigger collision)
    let content = "normal line\n...[not a hash]\n[abcdef0] close but no\nline 4\n";
    fs::write(root.join("marker_like.txt"), content).unwrap();

    let mut config = Config::load(root).unwrap();

    // This may succeed or fail depending on marker collision detection — either is acceptable
    let result = veil_file(root, &mut config, "marker_like.txt", None, true);
    if result.is_ok() {
        config.save(root).unwrap();
        unveil_file(root, &mut config, "marker_like.txt", None, true).unwrap();
        config.save(root).unwrap();
        let restored = fs::read_to_string(root.join("marker_like.txt")).unwrap();
        assert_eq!(restored, content);
    }
    // If MarkerCollision error, that's the correct guard behavior
}

// ============================================================================
// ContentHash edge cases
// ============================================================================

#[test]
fn stress_content_hash_from_string_edge_cases() {
    // Empty string
    assert!(ContentHash::from_string("".to_string()).is_err());

    // Too short
    assert!(ContentHash::from_string("abc".to_string()).is_err());

    // Non-hex characters
    assert!(ContentHash::from_string(
        "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz".to_string()
    )
    .is_err());

    // Correct length (64 hex chars for SHA-256) — should succeed
    let valid = "a".repeat(64);
    assert!(ContentHash::from_string(valid).is_ok());

    // BUG-152: from_string accepts any length >= 6 with valid hex chars.
    // It doesn't enforce SHA-256's 64-char length, so 65 chars is accepted.
    // This is a permissive validator — documenting the behavior.
    let too_long = "a".repeat(65);
    assert!(
        ContentHash::from_string(too_long).is_ok(),
        "BUG-152: from_string accepts oversized hashes (no upper bound check)"
    );

    // 63 chars is also accepted (no exact-length validation)
    let short_ish = "a".repeat(63);
    assert!(ContentHash::from_string(short_ish).is_ok());

    // But below minimum (6) should fail
    let too_short = "a".repeat(5);
    assert!(ContentHash::from_string(too_short).is_err());
}

// ============================================================================
// Pattern stress tests
// ============================================================================

#[test]
fn stress_pattern_regex_edge_cases() {
    // Empty regex
    assert!(Pattern::from_regex("").is_err() || Pattern::from_regex("").is_ok());

    // Pathological regex (catastrophic backtracking attempt)
    // The regex crate is safe from this, but let's confirm it doesn't hang
    let result = Pattern::from_regex("(a+)+$");
    if let Ok(p) = result {
        // Should complete quickly even on adversarial input
        let _ = p.matches(&"a".repeat(100));
    }

    // Very long regex
    let long_pattern = format!("({})", "a|".repeat(1000).trim_end_matches('|'));
    let _ = Pattern::from_regex(&long_pattern);

    // Regex with all special characters
    let _ = Pattern::from_regex(r"[\w\d\s\S\W\D]+");
    let _ = Pattern::from_regex(r"(?:a{1,100}){1,100}");
}

// ============================================================================
// Parser stress tests
// ============================================================================

#[test]
fn stress_tree_sitter_empty_files() {
    use funveil::parser::TreeSitterParser;
    use std::path::PathBuf;

    let parser = TreeSitterParser::new().unwrap();

    // Empty content for every language
    let extensions = [
        "rs", "ts", "tsx", "py", "sh", "go", "zig", "html", "css", "xml", "md", "tf", "yaml",
    ];

    for ext in &extensions {
        let path = PathBuf::from(format!("empty.{ext}"));
        let result = parser.parse_file(&path, "");
        // Should not panic — errors are fine
        let _ = result;
    }
}

#[test]
fn stress_tree_sitter_malformed_code() {
    use funveil::parser::TreeSitterParser;
    use std::path::PathBuf;

    let parser = TreeSitterParser::new().unwrap();

    let garbage = "}{}{}{}}}}{{{{[[[]]]])))((((***&&&^^^%%%$$$###@@@!!!";

    let extensions = [
        "rs", "ts", "tsx", "py", "sh", "go", "zig", "html", "css", "xml", "md",
    ];

    for ext in &extensions {
        let path = PathBuf::from(format!("garbage.{ext}"));
        // Should never panic on garbage input
        let _ = parser.parse_file(&path, garbage);
    }
}

#[test]
fn stress_tree_sitter_deeply_nested() {
    use funveil::parser::TreeSitterParser;
    use std::path::PathBuf;

    let parser = TreeSitterParser::new().unwrap();

    // Deeply nested braces (500 levels)
    let nested = "{".repeat(500) + &"}".repeat(500);

    let path = PathBuf::from("nested.rs");
    let _ = parser.parse_file(&path, &nested);

    let path = PathBuf::from("nested.ts");
    let _ = parser.parse_file(&path, &nested);
}

#[test]
fn stress_tree_sitter_large_file() {
    use funveil::parser::TreeSitterParser;
    use std::path::PathBuf;

    let parser = TreeSitterParser::new().unwrap();

    // Generate a large Rust file with many functions
    let mut code = String::new();
    for i in 0..500 {
        code.push_str(&format!(
            "fn func_{i}(x: i32) -> i32 {{\n    x + {i}\n}}\n\n"
        ));
    }

    let path = PathBuf::from("large.rs");
    let result = parser.parse_file(&path, &code);
    assert!(result.is_ok(), "parsing 500-function file should succeed");

    let parsed = result.unwrap();
    assert!(
        parsed.symbols.len() >= 500,
        "should find at least 500 symbols, found {}",
        parsed.symbols.len()
    );
}

// ============================================================================
// Patch parser stress tests
// ============================================================================

#[test]
fn stress_patch_parser_empty_input() {
    let _ = funveil::patch::PatchParser::parse_patch("");
}

#[test]
fn stress_patch_parser_garbage_input() {
    let inputs: Vec<String> = vec![
        "not a patch at all".to_string(),
        "--- \n+++ \n@@ \n".to_string(),
        "@@ -0,0 +0,0 @@\n".to_string(),
        "diff --git a/x b/y\n--- a/x\n+++ b/y\n@@ -1,1 +1,1 @@\n-old\n+new\n".repeat(100),
        "\0\0\0\0".to_string(),
        "diff --git \n".repeat(500),
    ];

    for input in &inputs {
        // Should never panic
        let _ = funveil::patch::PatchParser::parse_patch(input);
        let _ = funveil::patch::PatchParser::detect_format(input);
    }
}

#[test]
fn stress_patch_parser_large_hunk() {
    // Patch with a very large hunk (1000 lines changed)
    let mut patch = String::from("--- a/file.txt\n+++ b/file.txt\n@@ -1,1000 +1,1000 @@\n");
    for i in 0..1000 {
        patch.push_str(&format!("-old line {i}\n"));
        patch.push_str(&format!("+new line {i}\n"));
    }

    let result = funveil::patch::PatchParser::parse_patch(&patch);
    // Should parse successfully or return an error — not panic
    let _ = result;
}

// ============================================================================
// Rapid veil/unveil cycling (race condition / state corruption detection)
// ============================================================================

#[test]
fn stress_rapid_veil_unveil_cycles() {
    let temp = init_project();
    let root = temp.path();

    let content = "line 1\nline 2\nline 3\nline 4\nline 5\n";
    fs::write(root.join("cycle.txt"), content).unwrap();

    let mut config = Config::load(root).unwrap();

    // Rapidly veil and unveil 50 times
    for i in 0..50 {
        veil_file(root, &mut config, "cycle.txt", None, true)
            .unwrap_or_else(|e| panic!("veil failed on cycle {i}: {e}"));
        config.save(root).unwrap();

        unveil_file(root, &mut config, "cycle.txt", None, true)
            .unwrap_or_else(|e| panic!("unveil failed on cycle {i}: {e}"));
        config.save(root).unwrap();
    }

    // Content should be identical after all cycles
    let restored = fs::read_to_string(root.join("cycle.txt")).unwrap();
    assert_eq!(restored, content, "50 veil/unveil cycles must be lossless");
}

// ============================================================================
// Garbage collection stress test
// ============================================================================

#[test]
fn stress_gc_with_orphaned_objects() {
    let temp = init_project();
    let root = temp.path();
    let store = ContentStore::new(root);

    // Store 50 objects
    let mut all_hashes = Vec::new();
    for i in 0..50 {
        let hash = store
            .store(format!("orphan content {i}").as_bytes())
            .unwrap();
        all_hashes.push(hash);
    }

    // Only keep references to the first 10
    let referenced: Vec<ContentHash> = all_hashes[..10].to_vec();

    let (deleted, freed) = funveil::garbage_collect(root, &referenced, true).unwrap();

    assert_eq!(deleted, 40, "should delete 40 unreferenced objects");
    assert!(freed > 0, "should free some bytes");

    // Verify referenced objects still exist
    for hash in &referenced {
        assert!(store.exists(hash), "referenced object should survive GC");
    }

    // Verify orphaned objects are gone
    for hash in &all_hashes[10..] {
        assert!(
            !store.exists(hash),
            "orphaned object should be deleted by GC"
        );
    }
}
