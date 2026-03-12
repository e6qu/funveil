//! Fuzz the unified/git diff patch parser with arbitrary input.
//!
//! Targets: PatchParser::parse_patch, PatchParser::detect_format

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(input) = std::str::from_utf8(data) {
        // Should never panic regardless of input
        let _ = funveil::patch::parser::PatchParser::parse_patch(input);
        let _ = funveil::patch::parser::PatchParser::detect_format(input);
    }
});
