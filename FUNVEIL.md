# Funveil

> A lightweight tool for controlling file visibility in AI agent workspaces.

## Overview

Funveil creates a "veiled" view of a codebase where specific files or line ranges can be hidden from an AI agent while preserving the file structure and line numbers. This allows:

- **Focused agent work**: Show only relevant parts of a codebase
- **Secret protection**: Hide API keys, credentials, sensitive configuration
- **Gradual revelation**: Unveil sections as the agent needs them
- **Safety**: Prevent accidental modification of hidden content

## Philosophy

- **Simple**: Single directory, normal git, no kernel modules
- **Safe**: Write protection, checkpoints for recovery
- **Transparent**: Clear veil state, easy to inspect
- **Compatible**: Works with standard Unix tools

## How It Works

```
~/workspace/                    # Project directory (normal git repo)
├── .git/                       # Normal git repository
├── .funveil/                   # Funveil data
│   ├── config.yaml             # Current veil configuration
│   ├── objects/                # Hidden content storage
│   └── checkpoints/            # Saved veil states
├── api.py                      # File with veiled sections
└── secrets.env                 # Fully veiled file
```

**Veil operation:**
1. Extract hidden lines from file
2. Save to `.funveil/objects/`
3. Replace in file with `...` (preserving line count)
4. Set file read-only

**Unveil operation:**
1. Read hidden content from `.funveil/objects/`
2. Restore to file
3. Delete object
4. Make file writable (if no veils remain)

## Core Concepts

### Veil Types

**Full Veil**: Entire file hidden
```
secrets.env content: API_KEY=abc123
Veiled display: ...
```

**Partial Veil**: Specific line ranges hidden
```python
# api.py
1: import os
2: 
3: def public():
4:     ...
5: 
6:     ...
7:     return result
```
(Lines 4-6 veiled - first and last show `...`, middle blank)

### Line Preservation

Veiled files maintain the same line count:
- Hidden lines replaced with `...` (range boundaries)
- Middle lines replaced with blank lines
- Line numbers preserved for visible content
- File size changes (markers are smaller than original)

### Write Protection

Files with any veiled content are set read-only (`chmod 444`):
- Prevents accidental modification
- Must unveil before editing
- Works with all editors (standard Unix permissions)

## Commands

### Core Operations

```bash
fv init
    Initialize funveil in current directory
    Creates .funveil/ structure

fv status
    Show current veil state
    # Output:
    # Veiled files (2):
    #   - secrets.env
    # Partially veiled (1):
    #   - api.py (lines 10-20, 50-60)

fv veil <file>[:<start>-<end>]
    Hide file or line range
    # Examples:
    #   fv veil secrets.env
    #   fv veil api.py:10-20
    #   fv veil api.py:10-20,50-60

fv unveil <file>[:<start>-<end>]
    Reveal hidden content
    # Examples:
    #   fv unveil secrets.env
    #   fv unveil api.py:10-20
    #   fv unveil --all

fv restore
    Restore previous veil state
    # Use after 'unveil --all' and git commit

fv show <file>
    Display file with veil annotations
    # Shows which lines are veiled vs visible
```

### Checkpoints

```bash
fv checkpoint save <name>
    Save current veil state
    # Example: fv checkpoint save "stable"

fv checkpoint restore <name>
    Restore saved veil state
    # Example: fv checkpoint restore "stable"

fv checkpoint list
    Show all checkpoints

fv checkpoint show <name>
    Display checkpoint details
```

### Utilities

```bash
fv doctor
    Check veil integrity

fv clean
    Remove all funveil data (unveil first!)
```

## Use Scenarios

### Scenario 1: Hiding Secrets

```bash
# Clone repository with sensitive config
git clone https://github.com/company/api.git
cd api

# Initialize funveil
fv init

# Veil production secrets
fv veil config/production.env
fv veil .env

# Check status
fv status
# Fully veiled files:
#   - config/production.env
#   - .env

# Agent sees only ...
cat config/production.env
...

# File is protected
chmod config/production.env
# -r--r--r-- (read-only)
```

### Scenario 2: Agent Focused Development

```bash
# Start with minimal visibility
fv veil src/ --except src/public_api.py

# Agent can only see public API
# Implementation details are veiled

# As agent asks about specific implementations:
fv unveil src/core.py:50-100

# Agent can now see that section
# Continue gradually revealing as needed
```

### Scenario 3: Committing Changes

```bash
# Work with agent in veiled state...

# Before committing, unveil everything
fv unveil --all

# Review actual changes
git diff

# Commit
git add .
git commit -m "Add authentication endpoint"

# Restore veils for next session
fv restore
```

### Scenario 4: Safe Experimentation

```bash
# Save current working state
fv checkpoint save "working"

# Try different veil configuration
fv unveil --all
fv veil src/internal/

# Decide this doesn't work
fv checkpoint restore "working"

# Back to exactly where we were
```

### Scenario 5: Recovery

```bash
# Something went wrong
fv doctor
# Error: Object file missing

# Restore from checkpoint
fv checkpoint restore "morning-backup"

# Or restore to clean state
funveil unveil --all
rm -rf .funveil/
funveil init
```

## Data Formats

### Config: `.funveil/config.yaml`

```yaml
version: 1
veiled:
  files:                       # Fully veiled
    - secrets.env
    - config/production.yaml
  
  lines:                       # Partially veiled
    api.py:
      - [10, 20]
      - [50, 60]
    utils.py:
      - [5, 15]
```

### Objects: `.funveil/objects/`

Plain text files storing hidden content:
- `api.py.10-20` - Lines 10-20 of api.py
- `secrets.env` - Full content of secrets.env

Special characters escaped with `%`:
- `/` → `%2F`
- `:` → `%3A`

### Checkpoints: `.funveil/checkpoints/{name}/`

Complete snapshot:
```
checkpoints/stable/
├── config.yaml         # Veil configuration
├── objects/            # Hidden content
└── metadata.json       # Created timestamp, stats
```

## Implementation Summary

### Algorithms

**Veil Algorithm:**
1. Read original file
2. Extract hidden lines
3. Save to `.funveil/objects/{filename}.{start}-{end}`
4. Replace in file:
   - First line: `...\n`
   - Middle lines: `\n`
   - Last line: `...\n`
5. Set read-only: `chmod 444`
6. Update config

**Unveil Algorithm:**
1. Read object file
2. Restore content to working file
3. Delete object file
4. If no veils remain: `chmod 644`
5. Update config

**Checkpoint Save:**
1. Create directory `.funveil/checkpoints/{name}/`
2. Copy `config.yaml`
3. Copy `objects/` directory
4. Write `metadata.json`

**Checkpoint Restore:**
1. Auto-save current as `auto-before-restore`
2. Unveil all files
3. Copy checkpoint objects to `.funveil/objects/`
4. Copy checkpoint config
5. Re-apply all veils from config

### Error Handling

| Error | Handling |
|-------|----------|
| File not found | Clear error message, suggest checking path |
| Already veiled | Suggest using `show` to see current state |
| Not veiled | Suggest veiling first |
| Object missing | Offer checkpoint restore or unveil remaining |
| Permission denied | Explain write protection, suggest unveiling |

### Implementation Details

**Language:** Python 3.8+ or Go 1.18+

**Dependencies:**
- YAML parsing (PyYAML or Go yaml package)
- Standard library only for core functionality

**Estimated Size:** ~500-700 lines of code

**Testing:**
- Unit tests for each command
- Integration tests for workflows
- Property tests for line preservation
- Edge cases: empty files, binary files, unicode

## Trade-offs

### What We Get

- ✅ Simple implementation (~500 lines)
- ✅ No kernel modules or special privileges
- ✅ Works everywhere (Linux, macOS, containers)
- ✅ All core features (veil/unveil, line preservation, checkpoints)
- ✅ Standard Unix permissions for write protection

### What We Give Up

- ⚠️ Manual `unveil --all` / `restore` for git commits
- ⚠️ Git commits contain `...` if you forget to unveil
- ⚠️ 2x disk usage for veiled content (original + objects)
- ⚠️ No fine-grained access control (just read-only/not)

## Security Notes

- Objects stored as plain text (not encrypted)
- Same filesystem permissions as working files
- Included in `.gitignore` (not committed)
- Write protection uses Unix permissions (can be bypassed by owner)
- Not a security boundary, just protection against accidents

## License

AGPL-3.0 (see LICENSE file)

## Summary

Funveil provides a simple, effective way to control what an AI agent can see in a codebase:

1. **Initialize** once per project
2. **Veil** files or line ranges as needed
3. **Work** with agent in veiled directory
4. **Unveil all** before committing real changes
5. **Restore** veils after commit
6. **Checkpoint** important states for safety

The trade-off is minimal: a manual step before git commits, in exchange for a simple, reliable tool that works everywhere.
