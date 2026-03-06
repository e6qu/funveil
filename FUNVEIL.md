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
4. Set read-only

**Unveil operation:**
1. Read hidden content from `.funveil/objects/`
2. Restore to file
3. Delete object
4. Make file writable (if no veils remain)

## Core Concepts

### Two Configuration Modes

Funveil supports two complementary approaches to visibility control:

#### Mode 1: Veil (Blacklist)
**Default: Everything visible, veil specific items**

Use this when you want to hide specific secrets or implementation details while keeping most of the codebase visible.

```yaml
mode: veil
veiled:
  files:
    - secrets.env
    - config/production.yaml
  lines:
    api.py:
      - [10, 20]   # Hide implementation details
```

#### Mode 2: Unveil (Whitelist)
**Default: Everything hidden, unveil specific items**

Use this when you want to limit the agent to a minimal subset of the codebase.

```yaml
mode: unveil
unveiled:
  files:
    - README.md
    - src/public_api.py
  lines:
    core.py:
      - [1, 50]    # Only show first 50 lines
```

You can also combine both: start with whitelist mode, then apply additional veils on top.

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
fv init [--mode veil|unveil]
    Initialize funveil in current directory
    Creates .funveil/ structure
    Default mode: veil

fv status
    Show current veil state
    # Veil mode output:
    # Mode: veil (blacklist)
    # Fully veiled files (2):
    #   - secrets.env
    # Partially veiled (1):
    #   - api.py (lines 10-20, 50-60)
    # 
    # Unveil mode output:
    # Mode: unveil (whitelist)
    # Unveiled files (3):
    #   - README.md
    #   - src/public_api.py
    #   - core.py (lines 1-50)
    # Everything else: veiled

fv mode [veil|unveil]
    Show or change configuration mode
    # Show current mode:
    #   fv mode
    # Switch to whitelist mode:
    #   fv mode unveil
    # Switch to blacklist mode:
    #   fv mode veil

fv veil <file>[:<start>-<end>]
    Hide file or line range
    # In veil mode: adds to veiled list
    # In unveil mode: exception to whitelist (re-veils)
    # Examples:
    #   fv veil secrets.env
    #   fv veil api.py:10-20
    #   fv veil api.py:10-20,50-60

fv unveil <file>[:<start>-<end>]
    Reveal hidden content
    # In veil mode: removes from veiled list
    # In unveil mode: adds to unveiled list
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

### Scenario 1: Hiding Secrets (Veil Mode)

```bash
# Clone repository with sensitive config
git clone https://github.com/company/api.git
cd api

# Initialize in veil mode (default)
fv init
# or: fv init --mode veil

# Veil production secrets
fv veil config/production.env
fv veil .env

# Check status
fv status
# Mode: veil
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

### Scenario 2: Minimal Visibility (Unveil Mode)

```bash
# Initialize in unveil mode
fv init --mode unveil

# Unveil only what agent needs
fv unveil README.md
fv unveil src/public_api.py
fv unveil src/core.py:1-50

# Check status
fv status
# Mode: unveil (whitelist)
# Unveiled:
#   - README.md
#   - src/public_api.py
#   - src/core.py (lines 1-50)
# Everything else: veiled

# Agent can only see these specific files/sections
# All other files show as ...
```

### Scenario 3: Combined Mode (Whitelist + Exceptions)

```bash
# Start with whitelist mode - minimal visibility
fv init --mode unveil
fv unveil src/public_api.py

# But we also want to hide specific implementation details
# even within the unveiled file
fv veil src/public_api.py:80-120

# Status:
# Mode: unveil (whitelist)
# Unveiled:
#   - src/public_api.py (except lines 80-120)
# Everything else: veiled
```

### Scenario 4: Agent Focused Development

```bash
# Start with minimal visibility
fv init --mode unveil
fv unveil README.md

# As agent asks questions, gradually unveil
fv unveil src/core.py:1-50

# Agent needs to see implementation?
fv unveil src/core.py:51-100

# Continue unveiling sections as needed
# Agent never sees what you haven't explicitly unveiled
```

### Scenario 5: Committing Changes

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

### Scenario 6: Safe Experimentation

```bash
# Save current working state
fv checkpoint save "working"

# Try different veil configuration
fv mode unveil
fv unveil docs/

# Decide this doesn't work
fv checkpoint restore "working"

# Back to exactly where we were
```

### Scenario 7: Recovery

```bash
# Something went wrong
fv doctor
# Error: Object file missing

# Restore from checkpoint
fv checkpoint restore "morning-backup"

# Or restore to clean state
fv unveil --all
rm -rf .funveil/
fv init
```

## Data Formats

### Config: `.funveil/config.yaml`

#### Veil Mode (Blacklist)
```yaml
version: 1
mode: veil
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

#### Unveil Mode (Whitelist)
```yaml
version: 1
mode: unveil
unveiled:
  files:                       # Fully unveiled
    - README.md
    - src/public_api.py
  
  lines:                       # Partially unveiled
    core.py:
      - [1, 50]
    utils.py:
      - [1, 20]
      - [100, 150]

# In whitelist mode, everything not listed is veiled
# You can also add veiled exceptions on top:
veiled:
  lines:
    src/public_api.py:
      - [80, 120]              # Hide these lines even in unveiled file
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

### Determining What to Veil

**Veil Mode (Blacklist):**
```python
def is_veiled(file, line):
    if file in config.veiled.files:
        return True
    if file in config.veiled.lines:
        for start, end in config.veiled.lines[file]:
            if start <= line <= end:
                return True
    return False
```

**Unveil Mode (Whitelist):**
```python
def is_veiled(file, line):
    # First check if explicitly veiled (exception to whitelist)
    if file in config.veiled.files:
        return True
    if file in config.veiled.lines:
        for start, end in config.veiled.lines[file]:
            if start <= line <= end:
                return True
    
    # Then check if explicitly unveiled
    if file in config.unveiled.files:
        return False
    if file in config.unveiled.lines:
        for start, end in config.unveiled.lines[file]:
            if start <= line <= end:
                return False
    
    # Default: veiled in whitelist mode
    return True
```

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

**Apply Mode Algorithm:**
1. Scan all files in project (respecting .gitignore)
2. For each file, determine veiled lines based on mode:
   - Veil mode: veil only configured files/lines
   - Unveil mode: veil all except configured files/lines
3. Extract and store veiled content
4. Replace with markers in working files

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
5. Re-apply all veils from config based on mode

### Error Handling

| Error | Handling |
|-------|----------|
| File not found | Clear error message, suggest checking path |
| Already veiled/unveiled | Suggest using `show` to see current state |
| Not veiled | Suggest veiling first |
| Object missing | Offer checkpoint restore or unveil remaining |
| Permission denied | Explain write protection, suggest unveiling |

### Implementation Details

**Language:** Python 3.8+ or Go 1.18+

**Dependencies:**
- YAML parsing (PyYAML or Go yaml package)
- Gitignore parsing (optional, for file scanning)
- Standard library only for core functionality

**Estimated Size:** ~700-900 lines of code

**Testing:**
- Unit tests for each command
- Integration tests for workflows
- Property tests for line preservation
- Edge cases: empty files, binary files, unicode
- Mode switching tests

## Trade-offs

### What We Get

- ✅ Simple implementation (~700-900 lines)
- ✅ No kernel modules or special privileges
- ✅ Works everywhere (Linux, macOS, containers)
- ✅ All core features (veil/unveil, line preservation, checkpoints)
- ✅ Standard Unix permissions for write protection
- ✅ Two complementary modes (blacklist and whitelist)

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

1. **Initialize** once per project (veil or unveil mode)
2. **Configure** visibility (veil specific items, or unveil specific items)
3. **Work** with agent in veiled directory
4. **Unveil all** before committing real changes
5. **Restore** configuration after commit
6. **Checkpoint** important states for safety

The dual-mode design supports both:
- **Veil mode**: Hide secrets while keeping most code visible
- **Unveil mode**: Limit agent to minimal visible subset

The trade-off is minimal: a manual step before git commits, in exchange for a simple, reliable tool that works everywhere.
