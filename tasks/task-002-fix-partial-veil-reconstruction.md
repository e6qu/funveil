# Task 002: Fix Partial Veil Reconstruction Bug

**Priority**: P0 - CRITICAL  
**Severity**: HIGH  
**Type**: Data Corruption Bug  
**Estimated Time**: 3-4 hours  
**Dependencies**: None

## Problem Statement

The `unveil_file()` function corrupts files when reconstructing from partial veils by joining content chunks without preserving original line structure and missing content between non-contiguous ranges.

## Current Behavior

### Scenario

File `api.py` with content:
```
1:  # Header
2:  
3:  def public():
4:      pass
5:  
6:  # Implementation (veiled lines 6-10)
7:  def _helper():
8:      data = fetch()
9:      result = process(data)
10:     return result
11: 
12: # Exports (veiled lines 12-15)
13: __all__ = ['public']
14: 
15: # End
```

After veiling lines 6-10 and 12-15, then unveiling:

```python
# CURRENT (WRONG) - Lines are joined, content lost
def _helper():
    data = fetch()
    result = process(data)
    return result
__all__ = ['public']
# Lines 1-5, 11, 15+ are MISSING
```

### Code Location

**File**: `src/veil.rs:286-336`

```rust
// Sort ranges by start line
veiled_ranges.sort_by_key(|(r, _)| r.start());

// Reconstruct the file
let mut _current_line = 1;
for (range, content) in &veiled_ranges {
    // Add content for this range
    let content_str = String::from_utf8_lossy(content);
    full_content.push_str(&content_str);
    full_content.push('\n');
    _current_line = range.end() + 1;
}

fs::write(&file_path, full_content)?;
```

### Issues Identified

1. **Only veiled content is written**: Content outside veiled ranges is lost
2. **No gap handling**: Content between ranges (e.g., line 11) disappears
3. **Line structure lost**: Original formatting may be corrupted
4. **Unused tracking**: `_current_line` is computed but never used
5. **Wrong reconstruction approach**: Should merge veiled chunks with current visible content

## Expected Behavior

```python
# EXPECTED (CORRECT) - All content preserved
# Header

def public():
    pass

# Implementation (now visible)
def _helper():
    data = fetch()
    result = process(data)
    return result

# Exports (now visible)
__all__ = ['public']

# End
```

All original content should be present in correct order.

## Root Cause Analysis

The function attempts to reconstruct from veiled ranges only, but:

1. Partial veils store ONLY the veiled line content in CAS
2. Non-veiled lines remain in the working file (with markers)
3. Reconstruction needs to:
   - Read current file with veil markers
   - Replace markers with content from CAS
   - Preserve non-veiled lines

Current code does NOT do this - it only concatenates CAS content.

## Impact

**User Impact**: HIGH
- Data loss (content between veiled ranges disappears)
- File corruption (incorrect structure)
- Trust erosion

**Data Impact**: CRITICAL
- **IRREVERSIBLE** data loss if user commits corrupted file
- Original content not recoverable from CAS alone

**Security Impact**: LOW
- No security implications

**Frequency**: HIGH
- Every partial veil/unveil cycle with non-contiguous ranges

## Proposed Solution

### Algorithm

```
1. Read current file (with veil markers)
2. Parse to identify:
   - Veil markers: "...[hash]" or "..."
   - Visible content: all other lines
3. For each veiled range in config:
   a. Retrieve content from CAS
   b. Find corresponding marker lines in file
   c. Replace marker with actual content
4. Write reconstructed file
```

### Implementation

```rust
fn unveil_partial_veils(
    root: &Path,
    config: &mut Config,
    file: &str,
) -> Result<()> {
    let file_path = root.join(file);
    
    // Read current veiled file
    let veiled_content = fs::read_to_string(&file_path)?;
    let lines: Vec<&str> = veiled_content.lines().collect();
    
    // Get all partial veil ranges for this file
    let mut veiled_ranges: Vec<(LineRange, Vec<u8>)> = Vec::new();
    for key in config.objects.keys() {
        if let Some(pos) = key.find('#') {
            let obj_file = &key[..pos];
            if obj_file == file {
                let range_str = &key[pos + 1..];
                if let Ok(range) = LineRange::from_str(range_str) {
                    if let Some(meta) = config.get_object(key) {
                        let hash = ContentHash::from_string(meta.hash.clone());
                        let store = ContentStore::new(root);
                        if let Ok(content) = store.retrieve(&hash) {
                            veiled_ranges.push((range, content));
                        }
                    }
                }
            }
        }
    }
    
    // Sort by start line
    veiled_ranges.sort_by_key(|(r, _)| r.start());
    
    // Build output preserving structure
    let mut output = String::new();
    let mut line_idx = 0;
    let total_lines = lines.len();
    
    // Track which ranges we've processed
    let mut range_iter = veiled_ranges.iter().peekable();
    
    while line_idx < total_lines {
        let current_line = line_idx + 1; // 1-indexed
        
        // Check if current line is start of a veiled range
        if let Some((range, content)) = range_iter.peek() {
            if range.start() == current_line {
                // Found veiled range start
                let content_str = String::from_utf8_lossy(content);
                output.push_str(&content_str);
                output.push('\n');
                
                // Skip past veil marker lines in input
                // Veil markers occupy same number of lines as range
                let marker_lines = range.len();
                line_idx += marker_lines;
                
                range_iter.next();
                continue;
            }
        }
        
        // Not in a veiled range, add visible line as-is
        output.push_str(lines[line_idx]);
        output.push('\n');
        line_idx += 1;
    }
    
    // Write reconstructed content
    fs::write(&file_path, output)?;
    
    // Remove from config
    for (range, _) in &veiled_ranges {
        let key = format!("{}#{}", file, range);
        config.unregister_object(&key);
    }
    
    // Restore permissions if all veils removed
    if config.veiled_ranges(file)?.is_empty() {
        if let Some(meta) = config.objects.values().next() {
            let perms = u32::from_str_radix(&meta.permissions, 8).unwrap_or(0o644);
            let mut permissions = fs::metadata(&file_path)?.permissions();
            permissions.set_mode(perms);
            fs::set_permissions(&file_path, permissions)?;
        }
    }
    
    Ok(())
}
```

## Alternative Approach: Store Full Original

### Option B: Keep Full Copy on First Veil

```rust
// When first partial veil is applied:
// 1. Store FULL original file in CAS
// 2. Store hash as special key: "file.txt#_original"
// 3. On unveil, restore from full original

fn veil_partial(...) {
    // Store full original first time
    let original_key = format!("{}#_original", file);
    if config.get_object(&original_key).is_none() {
        let full_hash = store.store(content.as_bytes())?;
        config.register_object(
            original_key,
            ObjectMeta::new(full_hash, permissions)
        );
    }
    
    // Then proceed with partial veiling...
}

fn unveil_file(...) {
    // Check for full original
    let original_key = format!("{}#_original", file);
    if let Some(meta) = config.get_object(&original_key) {
        let hash = ContentHash::from_string(meta.hash.clone());
        let content = store.retrieve(&hash)?;
        fs::write(&file_path, content)?;
        // Remove all partial veils
        // Done!
        return Ok(());
    }
    
    // Fall back to marker-based reconstruction...
}
```

**Pros**:
- Simpler reconstruction
- Guaranteed no data loss
- Faster (single CAS read vs multiple)

**Cons**:
- Doubles storage for partially veiled files
- Need migration for existing configs

## Recommended Solution

**Use Option B (Store Full Original)** for these reasons:

1. **Simpler**: No complex marker parsing
2. **Safer**: Impossible to lose data
3. **Faster**: Single restore operation
4. **Disk is cheap**: Extra storage worth the safety

### Implementation Plan

1. Modify `veil_file()` to store full original on first partial veil
2. Modify `unveil_file()` to restore from full original
3. Handle migration of existing configs
4. Add cleanup for `_original` entries in `gc`

## Testing Requirements

### Unit Tests

```rust
#[test]
fn test_partial_unveil_preserves_all_content() {
    let temp = TempDir::new().unwrap();
    let content = "line1\nline2\nline3\nline4\nline5\nline6\n";
    fs::write(temp.path().join("test.txt"), content).unwrap();
    
    let mut config = Config::new(Mode::Blacklist);
    veil_file(temp.path(), &mut config, "test.txt", 
        Some(&[LineRange::new(2, 3).unwrap()])).unwrap();
    
    unveil_file(temp.path(), &mut config, "test.txt", None).unwrap();
    
    let restored = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert_eq!(restored, content);
}

#[test]
fn test_multiple_partial_veils_unveil() {
    // Veil ranges 5-10 and 20-25, then unveil
    // Verify all content present
}

#[test]
fn test_partial_unveil_preserves_formatting() {
    // Verify indentation, blank lines, etc. preserved
}

#[test]
fn test_original_stored_on_first_partial_veil() {
    // Verify "file#_original" key created
}
```

### E2E Tests

```bash
# Create test file
cat > test.txt << 'EOF'
header
content1
content2  
content3
footer
EOF

# Veil middle section
fv init --mode blacklist
fv veil test.txt#2-4

# Verify markers
grep "...\\[" test.txt

# Unveil
fv unveil test.txt

# Verify content matches original
diff test.txt test.txt.bak
```

## Acceptance Criteria

- [ ] Unveiling partially veiled file preserves ALL content
- [ ] Non-contiguous ranges handled correctly
- [ ] Formatting preserved (indentation, blank lines)
- [ ] Original stored on first partial veil
- [ ] Unit tests for all scenarios
- [ ] E2E test for round-trip integrity
- [ ] Migration path for existing configs
- [ ] GC cleans up `_original` entries

## Migration Strategy

For existing configs without `_original`:

1. **Best effort**: Reconstruct from markers
2. **Fallback**: Warn user, keep current (possibly corrupt) content
3. **Manual**: User can re-create file from git/VCS

```rust
fn unveil_file(...) {
    let original_key = format!("{}#_original", file);
    
    if config.get_object(&original_key).is_some() {
        // Easy path: restore from original
        restore_from_original(...)
    } else {
        // Legacy path: try marker reconstruction
        eprintln!("Warning: Partial veil created before fix. \
                   Content between ranges may be lost.");
        reconstruct_from_markers(...)
    }
}
```

## Notes

- This is a **data corruption bug** - prioritize over all other tasks
- Consider adding `--dry-run` to `unveil` to preview restoration
- Document the `_original` storage behavior
- Update SPEC.md to document partial veil storage strategy
