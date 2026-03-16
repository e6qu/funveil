use crate::config::Config;
use crate::types::{ConfigKey, ContentHash};

pub struct DoctorReport {
    pub issues: Vec<String>,
}

pub fn check_integrity(
    config: &Config,
    cas_has: impl Fn(&ContentHash) -> bool,
    file_exists: impl Fn(&str) -> bool,
    is_legacy: impl Fn(&str) -> bool,
    metadata_exists: impl Fn(&ContentHash) -> bool,
) -> DoctorReport {
    let mut issues = Vec::new();

    for (key, meta) in &config.objects {
        let hash = match ContentHash::from_string(meta.hash.clone()) {
            Ok(h) => h,
            Err(e) => {
                issues.push(format!("Invalid hash for {key}: {e}"));
                continue;
            }
        };
        if !cas_has(&hash) {
            issues.push(format!("Missing object: {key}"));
        }

        let parsed_key = ConfigKey::parse(key);
        if let ConfigKey::FullVeil { file } = parsed_key {
            if file_exists(file) && is_legacy(file) {
                issues.push(format!(
                    "Legacy marker detected: {file} (run `fv apply` to migrate)"
                ));
            }
            if crate::config::is_supported_source(std::path::Path::new(file))
                && !metadata_exists(&hash)
            {
                issues.push(format!(
                    "Missing metadata for {file} (run `fv apply` to rebuild)"
                ));
            }
        }
    }

    DoctorReport { issues }
}

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ObjectMeta;
    use std::collections::HashMap;

    fn valid_hash_string() -> String {
        "a".repeat(64)
    }

    fn config_with_objects(objects: Vec<(&str, &str)>) -> Config {
        let mut map = HashMap::new();
        for (key, hash) in objects {
            map.insert(
                key.to_string(),
                ObjectMeta {
                    hash: hash.to_string(),
                    permissions: "0644".to_string(),
                    owner: None,
                },
            );
        }
        Config {
            objects: map,
            ..Config::default()
        }
    }

    #[test]
    fn empty_config_no_issues() {
        let config = Config::default();
        let report = check_integrity(&config, |_| true, |_| false, |_| false, |_| true);
        assert!(report.issues.is_empty());
    }

    #[test]
    fn valid_hash_cas_present_no_issues() {
        let hash = valid_hash_string();
        let config = config_with_objects(vec![("src/main.rs", &hash)]);
        let report = check_integrity(&config, |_| true, |_| false, |_| false, |_| true);
        assert!(report.issues.is_empty());
    }

    #[test]
    fn invalid_hash_reported() {
        let config = config_with_objects(vec![("src/main.rs", "bad")]);
        let report = check_integrity(&config, |_| true, |_| false, |_| false, |_| true);
        assert_eq!(report.issues.len(), 1);
        assert!(report.issues[0].contains("Invalid hash"));
        assert!(report.issues[0].contains("src/main.rs"));
    }

    #[test]
    fn missing_cas_object_reported() {
        let hash = valid_hash_string();
        let config = config_with_objects(vec![("src/main.rs", &hash)]);
        let report = check_integrity(&config, |_| false, |_| false, |_| false, |_| true);
        assert_eq!(report.issues.len(), 1);
        assert!(report.issues[0].contains("Missing object"));
    }

    #[test]
    fn legacy_marker_detected() {
        let hash = valid_hash_string();
        let config = config_with_objects(vec![("src/main.rs", &hash)]);
        let report = check_integrity(&config, |_| true, |_| true, |_| true, |_| true);
        assert!(report.issues.iter().any(|i| i.contains("Legacy marker")));
    }

    #[test]
    fn missing_metadata_for_supported_source() {
        let hash = valid_hash_string();
        let config = config_with_objects(vec![("src/main.rs", &hash)]);
        let report = check_integrity(&config, |_| true, |_| false, |_| false, |_| false);
        assert!(report.issues.iter().any(|i| i.contains("Missing metadata")));
    }
}
