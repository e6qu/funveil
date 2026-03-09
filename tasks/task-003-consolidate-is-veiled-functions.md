# Task 003: Consolidate Duplicate `is_veiled` Functions

**Priority**: P1 - HIGH  
**Severity**: MEDIUM  
**Type**: Code Duplication / API Confusion  
**Estimated Time**: 1-2 hours  
**Dependencies**: None

## Problem Statement

Two functions with similar names exist with different semantics, causing potential confusion and misuse.

## Current State

### Function 1: `Config::is_veiled`
**File**: `src/config.rs:171-216`  
**Signature**: `pub fn is_veiled(&self, file: &str, line: usize) -> Result<bool>`  
**Purpose**: Check if a specific LINE in a file is veiled based on config mode

```rust
impl Config {
    pub fn is_veiled(&self, file: &str, line: usize) -> Result<bool> {
        let blacklist = self.parsed_blacklist()?;
        let whitelist = self.parsed_whitelist()?;
        
        match self.mode {
            Mode::Blacklist => {
                for entry in &blacklist {
                    if entry.pattern.matches(file) {
                        if let Some(ranges) = &entry.ranges {
                            return Ok(ranges.iter().any(|r| r.contains(line)));
                        }
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Mode::Whitelist => {
                // ... complex logic
            }
        }
    }
}
```

### Function 2: `veil::is_veiled`
**File**: `src/veil.rs:472-478`  
**Signature**: `pub fn is_veiled(config: &Config, file: &str) -> bool`  
**Purpose**: Check if a FILE has ANY veils registered (full or partial)

```rust
pub fn is_veiled(config: &Config, file: &str) -> bool {
    config.get_object(file).is_some()
        || config
            .objects
            .keys()
            .any(|k| k.starts_with(&format!("{file}#")))
}
```

## Issues

1. **Name collision**: Same base name, different semantics
2. **Import confusion**: `use veil::is_veiled` vs `config.is_veiled()`
3. **Documentation gap**: No clear guidance on which to use when
4. **Type mismatch**: One returns `Result<bool>`, other returns `bool`
5. **Scope mismatch**: One checks line-level, other checks file-level

## Usage Analysis

### `Config::is_veiled` Usage
```rust
// src/main.rs:834
if let Ok(veiled) = config.is_veiled(&file, line_num) {
    is_veiled = veiled;
}

// tests/integration_test.rs:154
assert!(config.is_veiled("secrets.env", 1).unwrap());
```

### `veil::is_veiled` Usage  
```rust
// src/main.rs:770
if is_veiled(&config, &path_str) {
    let _ = unveil_file(&root, &mut config, &path_str, None);
}

// src/main.rs:788
if is_veiled(&config, &pattern) {
    unveil_file(&root, &mut config, &pattern, None)?;
}
```

## Proposed Solution

### Option A: Rename `veil::is_veiled` (Recommended)

```rust
// src/veil.rs
/// Check if a file has any veils registered (full or partial)
pub fn has_veils(config: &Config, file: &str) -> bool {
    config.get_object(file).is_some()
        || config
            .objects
            .keys()
            .any(|k| k.starts_with(&format!("{file}#")))
}
```

Update all call sites:

```rust
// src/main.rs:770
if has_veils(&config, &path_str) {
    let _ = unveil_file(&root, &mut config, &path_str, None);
}

// src/main.rs:788  
if has_veils(&config, &pattern) {
    unveil_file(&root, &mut config, &pattern, None)?;
}
```

Update exports:

```rust
// src/lib.rs
pub use veil::{has_veils, is_veiled, unveil_all, unveil_file, veil_file};
```

### Option B: Rename `Config::is_veiled`

Less desirable as it's a method and used more widely.

```rust
impl Config {
    pub fn is_line_veiled(&self, file: &str, line: usize) -> Result<bool> {
        // ...
    }
}
```

### Option C: Document Distinction

Keep both but add clear documentation:

```rust
/// Check if a specific line in a file is veiled according to config rules.
/// 
/// # Arguments
/// * `file` - Relative file path
/// * `line` - 1-indexed line number
/// 
/// # Returns
/// * `Ok(true)` - Line is veiled
/// * `Ok(false)` - Line is visible  
/// * `Err(_)` - Config parsing error
pub fn is_veiled(&self, file: &str, line: usize) -> Result<bool>

/// Check if a file has any veils registered in the objects map.
///
/// This is a quick check that doesn't parse config patterns.
/// Returns true if file is fully veiled OR has partial veils.
pub fn has_veils(config: &Config, file: &str) -> bool
```

## Recommended: Option A + Option C

1. Rename `veil::is_veiled` to `has_veils`
2. Add comprehensive documentation to both
3. Add examples in doc comments

## Implementation

### Step 1: Rename in veil.rs

```rust
// src/veil.rs

/// Check if a file has any veils (full or partial).
///
/// This performs a quick lookup in the objects map without parsing
/// config patterns. Use this to check if unveiling is needed.
///
/// # Arguments
/// * `config` - Funveil configuration
/// * `file` - Relative file path to check
///
/// # Returns
/// `true` if file is fully veiled OR has partial veils, `false` otherwise
///
/// # Example
/// ```
/// let config = Config::load(root)?;
/// if has_veils(&config, "secrets.env") {
///     unveil_file(root, &mut config, "secrets.env", None)?;
/// }
/// ```
pub fn has_veils(config: &Config, file: &str) -> bool {
    config.get_object(file).is_some()
        || config
            .objects
            .keys()
            .any(|k| k.starts_with(&format!("{file}#")))
}

// Keep old name as deprecated alias
#[deprecated(since = "0.2.0", note = "Use `has_veils` instead")]
pub fn is_veiled(config: &Config, file: &str) -> bool {
    has_veils(config, file)
}
```

### Step 2: Update lib.rs

```rust
// src/lib.rs
pub use veil::{has_veils, is_veiled, unveil_all, unveil_file, veil_file};
```

### Step 3: Update main.rs

```rust
// src/main.rs
use funveil::{has_veils, is_veiled, unveil_all, unveil_file, veil_file, /* ... */};

// Line 770
if has_veils(&config, &path_str) {
    let _ = unveil_file(&root, &mut config, &path_str, None);
}

// Line 788
if has_veils(&config, &pattern) {
    unveil_file(&root, &mut config, &pattern, None)?;
}
```

### Step 4: Add Config::is_veiled documentation

```rust
// src/config.rs

impl Config {
    /// Check if a specific line in a file is veiled.
    ///
    /// This parses config patterns and applies mode logic to determine
    /// if a specific line should be hidden.
    ///
    /// # Arguments
    /// * `file` - Relative file path
    /// * `line` - 1-indexed line number to check
    ///
    /// # Returns
    /// * `Ok(true)` - Line is veiled according to config
    /// * `Ok(false)` - Line is visible
    /// * `Err` - Config parsing error (invalid pattern, etc.)
    ///
    /// # Example
    /// ```
    /// let config = Config::load(root)?;
    /// if config.is_veiled("api.py", 42)? {
    ///     println!("Line 42 is veiled");
    /// }
    /// ```
    ///
    /// # See Also
    /// * [`has_veils`](crate::has_veils) - Quick check if file has any veils
    pub fn is_veiled(&self, file: &str, line: usize) -> Result<bool> {
        // ... existing implementation
    }
}
```

## Testing Requirements

### Unit Tests

```rust
#[test]
fn test_has_veils_full_veil() {
    let mut config = Config::new(Mode::Blacklist);
    config.register_object(
        "secrets.env".to_string(),
        ObjectMeta::new(ContentHash::from_content(b"test"), 0o644)
    );
    
    assert!(has_veils(&config, "secrets.env"));
}

#[test]
fn test_has_veils_partial_veil() {
    let mut config = Config::new(Mode::Blacklist);
    config.register_object(
        "api.py#10-20".to_string(),
        ObjectMeta::new(ContentHash::from_content(b"test"), 0o644)
    );
    
    assert!(has_veils(&config, "api.py"));
}

#[test]
fn test_has_veils_no_veil() {
    let config = Config::new(Mode::Blacklist);
    
    assert!(!has_veils(&config, "visible.txt"));
}

#[test]
fn test_config_is_veiled_line_level() {
    let mut config = Config::new(Mode::Blacklist);
    config.add_to_blacklist("api.py#10-20");
    
    assert!(config.is_veiled("api.py", 15).unwrap());
    assert!(!config.is_veiled("api.py", 5).unwrap());
}
```

### Doc Tests

Add examples in documentation that compile and run as tests.

## Acceptance Criteria

- [ ] `veil::is_veiled` renamed to `has_veils`
- [ ] Old name deprecated with warning
- [ ] All call sites updated to use `has_veils`
- [ ] Comprehensive documentation for both functions
- [ ] Doc examples added
- [ ] Unit tests for `has_veils`
- [ ] No compilation warnings
- [ ] CHANGELOG.md updated

## Migration Guide

For users with custom code:

```markdown
## v0.2.0 Breaking Changes

### `is_veiled` renamed to `has_veils`

The function `veil::is_veiled(config, file)` has been renamed to `has_veils(config, file)` 
to avoid confusion with `Config::is_veiled(file, line)`.

**Before:**
```rust
if is_veiled(&config, "file.txt") {
    unveil_file(...)?;
}
```

**After:**
```rust
if has_veils(&config, "file.txt") {
    unveil_file(...)?;
}
```

The old name is deprecated and will be removed in v0.3.0.
```

## Notes

- This is a **non-breaking change** with deprecation warning
- Can be removed entirely in v0.3.0
- Consider adding to public API documentation
- Update SPEC.md with clarified semantics
