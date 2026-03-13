//! Fuzz config loading from arbitrary YAML content.
//!
//! Target: Config deserialization, parsed_blacklist, parsed_whitelist

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::fs;
use tempfile::TempDir;

fuzz_target!(|data: &[u8]| {
    if let Ok(yaml_content) = std::str::from_utf8(data) {
        // Limit input size
        if yaml_content.len() > 50_000 {
            return;
        }

        let temp_dir = match TempDir::new() {
            Ok(d) => d,
            Err(_) => return,
        };

        let config_path = temp_dir.path().join(".funveil_config");
        if fs::write(&config_path, yaml_content).is_err() {
            return;
        }

        // Config::load should never panic on malformed YAML
        if let Ok(config) = funveil::config::Config::load(temp_dir.path()) {
            // Parsing blacklist/whitelist entries should never panic
            let _ = config.parsed_blacklist();
            let _ = config.parsed_whitelist();

            // Querying veiled status should never panic
            let _ = config.is_veiled("test.rs", 1);
            let _ = config.veiled_ranges("test.rs");
        }
    }
});
