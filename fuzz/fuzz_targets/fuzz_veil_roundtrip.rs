//! Fuzz the veil/unveil roundtrip with arbitrary file content and range specs.
//!
//! Verifies that veil -> unveil produces the original content (or returns an error).

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use std::fs;
use tempfile::TempDir;

#[derive(Arbitrary, Debug)]
struct VeilInput {
    /// File content to veil
    content: String,
    /// Whether to do a partial veil
    partial: bool,
    /// Range start (1-indexed) for partial veils
    range_start: u8,
    /// Range end (1-indexed) for partial veils
    range_end: u8,
}

fuzz_target!(|input: VeilInput| {
    // Limit content size and skip empty
    if input.content.is_empty() || input.content.len() > 10_000 {
        return;
    }

    // Skip content with control characters (validate_filename would reject)
    if input.content.bytes().any(|b| b < 0x09 || (b > 0x0d && b < 0x20)) {
        return;
    }

    let temp_dir = match TempDir::new() {
        Ok(d) => d,
        Err(_) => return,
    };

    // Initialize funveil project
    let root = temp_dir.path();
    let data_dir = root.join(".funveil");
    let cas_dir = data_dir.join("objects");
    if fs::create_dir_all(&cas_dir).is_err() {
        return;
    }

    let mut config = match funveil::config::Config::load(root) {
        Ok(c) => c,
        Err(_) => return,
    };

    // Write test file
    let file_name = "fuzz_test.txt";
    let file_path = root.join(file_name);
    if fs::write(&file_path, &input.content).is_err() {
        return;
    }

    // Build ranges for partial veil
    let ranges = if input.partial {
        let start = (input.range_start as usize).max(1);
        let end = (input.range_end as usize).max(start);
        match funveil::types::LineRange::new(start, end) {
            Ok(r) => Some(vec![r]),
            Err(_) => None,
        }
    } else {
        None
    };

    let range_slice = ranges.as_deref();

    // Veil the file
    let veil_result = funveil::veil_file(root, &mut config, file_name, range_slice, true);
    if veil_result.is_err() {
        return; // Errors are acceptable — panics are not
    }
    let _ = config.save(root);

    // Unveil the file
    let unveil_result = funveil::unveil_file(root, &mut config, file_name, range_slice, true);
    if unveil_result.is_err() {
        return; // Errors are acceptable — panics are not
    }
    let _ = config.save(root);

    // For full veils, verify roundtrip produces original content
    if !input.partial {
        if let Ok(restored) = fs::read_to_string(&file_path) {
            assert_eq!(
                restored, input.content,
                "Full veil roundtrip lost data"
            );
        }
    }
});
