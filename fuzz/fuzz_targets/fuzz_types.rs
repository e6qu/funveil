//! Fuzz core type parsing and validation.
//!
//! Targets: ContentHash::from_string, ConfigEntry::parse, Pattern::from_regex,
//!          LineRange::new, LineRange::from_str

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::str::FromStr;

fuzz_target!(|data: &[u8]| {
    if let Ok(input) = std::str::from_utf8(data) {
        // ContentHash validation — should never panic
        let _ = funveil::types::ContentHash::from_string(input.to_string());

        // ConfigEntry parsing — should never panic
        let _ = funveil::types::ConfigEntry::parse(input);

        // Pattern::from_regex — should never panic (even on pathological regexes)
        let _ = funveil::types::Pattern::from_regex(input);

        // LineRange from string — should never panic
        let _ = funveil::types::LineRange::from_str(input);

        // LineRange with arbitrary bounds
        if input.len() >= 8 {
            let bytes = input.as_bytes();
            let start = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
            let end = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
            let _ = funveil::types::LineRange::new(start, end);
        }
    }
});
