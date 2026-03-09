# Task 001: Implement or Remove Unimplemented CLI Commands

**Priority**: P0 - CRITICAL  
**Severity**: HIGH  
**Type**: Incomplete Implementation  
**Estimated Time**: 4-6 hours  
**Dependencies**: None

## Problem Statement

Nine CLI commands are documented in help and README but have empty implementations that silently do nothing.

## Current Behavior

```bash
$ fv apply
Re-applying veils...
# Command exits successfully but does nothing

$ fv restore
Restoring previous state...
# Command exits successfully but does nothing

$ fv checkpoint save test
Saving checkpoint: test
# Command exits successfully but does nothing

$ fv gc
Running garbage collection...
# Command exits successfully but does nothing
```

## Expected Behavior

Each command should either:
1. **Work as documented**: Implement the full functionality
2. **Fail with clear error**: Return error code with message "Command not yet implemented"
3. **Be removed**: Remove from CLI, help text, and documentation

## Affected Files

| File | Lines | Issue |
|------|-------|-------|
| `src/main.rs` | 803 | `apply` - Empty implementation |
| `src/main.rs` | 810 | `restore` - Empty implementation |
| `src/main.rs` | 863 | `checkpoint save` - Empty implementation |
| `src/main.rs` | 869 | `checkpoint restore` - Empty implementation |
| `src/main.rs` | 875 | `checkpoint list` - Empty implementation |
| `src/main.rs` | 881 | `checkpoint show` - Empty implementation |
| `src/main.rs` | 917 | `gc` - Empty implementation |
| `src/main.rs` | 924 | `clean` - Empty implementation |
| `README.md` | 129-136 | Documents commands that don't work |

## Impact

**User Impact**: HIGH
- Users expect commands to work based on help text
- Silent failures make debugging impossible
- Erodes trust in the tool

**Data Impact**: NONE
- Commands do nothing, so no data corruption

**Security Impact**: NONE

## Root Cause

Development was incomplete when commands were added to CLI interface. The pattern shows:
1. Command added to `enum Commands` 
2. Command handler created with placeholder
3. TODO comment left
4. Documentation written as if feature exists

## Proposed Solution

### Option A: Full Implementation (Recommended)

Implement all 9 commands with full functionality:

1. **`apply`**: Re-apply all veils from config
2. **`restore`**: Restore from last checkpoint
3. **`checkpoint save`**: Use existing `save_checkpoint` function
4. **`checkpoint restore`**: Implement checkpoint restoration
5. **`checkpoint list`**: Use existing `list_checkpoints` function
6. **`checkpoint show`**: Use existing `show_checkpoint` function  
7. **`gc`**: Implement garbage collection for CAS
8. **`clean`**: Remove all .funveil data

### Option B: Mark as Unimplemented

Replace each TODO with:

```rust
Commands::Apply => {
    eprintln!("Error: 'apply' command is not yet implemented");
    std::process::exit(1);
}
```

Update help text to show: `apply (not implemented)`

### Option C: Remove Commands

Remove from:
- `src/main.rs`: Remove enum variants and handlers
- `README.md`: Remove from command list
- Help text: Will auto-update

## Recommended Approach: Option A (Hybrid)

1. **Implement now** (have backend functions):
   - `checkpoint save` → `save_checkpoint()`
   - `checkpoint list` → `list_checkpoints()` 
   - `checkpoint show` → `show_checkpoint()`

2. **Implement with effort**:
   - `gc` → Call `garbage_collect()` from cas.rs
   - `clean` → Delete `.funveil` and `.funveil_config`
   - `apply` → Re-run veil_file on all objects in config

3. **Defer with clear error**:
   - `restore` → Needs full checkpoint system
   - `checkpoint restore` → Needs diff engine

## Implementation Steps

### Step 1: Quick Wins (30 min)

```rust
// src/main.rs - checkpoint commands
Commands::Checkpoint { cmd } => {
    match cmd {
        CheckpointCmd::Save { name } => {
            let config = Config::load(&root)?;
            save_checkpoint(&root, &config, &name)?;
        }
        CheckpointCmd::List => {
            let checkpoints = list_checkpoints(&root)?;
            for name in checkpoints {
                println!("  - {}", name);
            }
        }
        CheckpointCmd::Show { name } => {
            show_checkpoint(&root, &name)?;
        }
        CheckpointCmd::Restore { name } => {
            return Err(anyhow::anyhow!(
                "checkpoint restore is not yet implemented"
            ));
        }
    }
}
```

### Step 2: GC Implementation (1 hour)

```rust
Commands::Gc => {
    let config = Config::load(&root)?;
    let referenced: Vec<ContentHash> = config.objects
        .values()
        .map(|m| ContentHash::from_string(m.hash.clone()))
        .collect();
    
    let (deleted, freed) = garbage_collect(&root, &referenced)?;
    
    if !quiet {
        println!("Garbage collected {} objects", deleted);
        println!("Freed {} bytes", freed);
    }
}
```

### Step 3: Clean Implementation (30 min)

```rust
Commands::Clean => {
    let _ = fs::remove_dir_all(root.join(".funveil"));
    let _ = fs::remove_file(root.join(CONFIG_FILE));
    
    if !quiet {
        println!("Removed all funveil data");
    }
}
```

### Step 4: Apply Implementation (2 hours)

```rust
Commands::Apply => {
    let config = Config::load(&root)?;
    
    for (key, meta) in &config.objects {
        let path = root.join(key);
        if path.exists() {
            // Re-apply veil
            let content = fs::read_to_string(&path)?;
            let store = ContentStore::new(&root);
            let hash = store.store(content.as_bytes())?;
            
            // Verify hash matches
            if hash.full() != meta.hash {
                eprintln!("Warning: {} content changed", key);
            }
        }
    }
}
```

### Step 5: Restore Implementation (4+ hours)

Requires:
- Loading checkpoint manifest
- Restoring each file from CAS
- Handling conflicts with current state
- Updating config

**Defer to separate task** if time-constrained.

## Testing Requirements

### Unit Tests

```rust
#[test]
fn test_checkpoint_save_creates_manifest() {
    let temp = TempDir::new().unwrap();
    let config = setup_test_config(&temp);
    
    save_checkpoint(&temp.path(), &config, "test-cp").unwrap();
    
    assert!(temp.path().join(".funveil/checkpoints/test-cp/manifest.yaml").exists());
}

#[test]
fn test_gc_removes_unreferenced_objects() {
    let temp = TempDir::new().unwrap();
    let store = ContentStore::new(temp.path());
    
    // Store content
    let hash = store.store(b"test").unwrap();
    
    // Run GC with no references
    let (deleted, _) = garbage_collect(temp.path(), &[]).unwrap();
    
    assert_eq!(deleted, 1);
    assert!(!store.exists(&hash));
}
```

### E2E Tests

```bash
# Test checkpoint save
fv init
echo "test" > test.txt
fv checkpoint save test-1
fv checkpoint list | grep test-1

# Test gc
fv veil test.txt
rm .funveil/objects/ab/cd/ef... # Remove object manually
fv gc # Should report error or cleanup

# Test clean
fv clean
test ! -d .funveil
test ! -f .funveil_config
```

## Acceptance Criteria

- [ ] `checkpoint save` works and creates manifest
- [ ] `checkpoint list` shows saved checkpoints
- [ ] `checkpoint show` displays checkpoint details
- [ ] `checkpoint restore` either works or fails with clear message
- [ ] `gc` removes unreferenced CAS objects
- [ ] `clean` removes all funveil data
- [ ] `apply` re-applies veils to modified files
- [ ] `restore` either works or fails with clear message
- [ ] All commands have unit tests
- [ ] All commands have e2e tests
- [ ] README.md accurately reflects working commands
- [ ] No silent failures

## Rollback Plan

If implementation reveals deeper issues:
1. Revert to Option B (fail with error message)
2. Create separate tasks for complex features
3. Document as "planned" in README

## Notes

- Checkpoint functions already exist in `src/checkpoint.rs`
- GC function already exists in `src/cas.rs`  
- This is primarily wiring existing code to CLI
- `restore` is most complex, can be deferred
