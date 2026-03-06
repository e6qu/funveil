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

#### Mode 1: Blacklist
**Default: Everything visible, blacklist specific items**

Use this when you want to hide specific secrets or implementation details while keeping most of the codebase visible.

```yaml
mode: blacklist
blacklist:
  - secrets.env                 # Hide entire file
  - config/production.yaml      # Hide entire file
  - api.py#10-20                # Hide lines 10-20 only
  - src/internal/               # Hide entire directory
```

#### Mode 2: Whitelist
**Default: Everything hidden, whitelist specific items**

Use this when you want to limit the agent to a minimal subset of the codebase.

```yaml
mode: whitelist
whitelist:
  - README.md                   # Show entire file
  - src/public_api.py           # Show entire file
  - core.py#1-50                # Show lines 1-50 only
  - utils.py#1-20,100-150       # Show multiple ranges
```

You can also combine both: start with whitelist mode, then apply a blacklist on top.

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
fv init [--mode blacklist|whitelist]
    Initialize funveil in current directory
    Creates .funveil/ structure
    Default mode: blacklist

fv status
    Show current veil state
    # Blacklist mode output:
    # Mode: blacklist
    # Blacklisted (3):
    #   - secrets.env (full)
    #   - config/production.yaml (full)
    #   - api.py#10-20,50-60
    # 
    # Whitelist mode output:
    # Mode: whitelist
    # Whitelisted (3):
    #   - README.md (full)
    #   - src/public_api.py (full)
    #   - core.py#1-50
    # Everything else: veiled

fv mode [blacklist|whitelist]
    Show or change configuration mode
    # Show current mode:
    #   fv mode
    # Switch to whitelist mode:
    #   fv mode whitelist
    # Switch to blacklist mode:
    #   fv mode blacklist

fv veil <path>[#<start>-<end>[,<start>-<end>]]
    Hide file, directory, or line range
    # In blacklist mode: adds to blacklist
    # In whitelist mode: adds to blacklist (exception)
    # Examples:
    #   fv veil secrets.env
    #   fv veil api.py#10-20
    #   fv veil api.py#10-20,50-60
    #   fv veil src/internal/

fv unveil <path>[#<start>-<end>[,<start>-<end>]]
    Reveal hidden content
    # In blacklist mode: removes from blacklist
    # In whitelist mode: adds to whitelist
    # Examples:
    #   fv unveil secrets.env
    #   fv unveil api.py#10-20
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

### Scenario 1: Hiding Secrets (Blacklist Mode)

```bash
# Clone repository with sensitive config
git clone https://github.com/company/api.git
cd api

# Initialize in blacklist mode (default)
fv init
# or: fv init --mode blacklist

# Blacklist production secrets
fv veil config/production.env
fv veil .env

# Check status
fv status
# Mode: blacklist
# Blacklisted:
#   - config/production.env (full)
#   - .env (full)

# Agent sees only ...
cat config/production.env
...

# File is protected
chmod config/production.env
# -r--r--r-- (read-only)
```

### Scenario 2: Minimal Visibility (Whitelist Mode)

```bash
# Initialize in whitelist mode
fv init --mode whitelist

# Whitelist only what agent needs
fv unveil README.md
fv unveil src/public_api.py
fv unveil src/core.py#1-50

# Check status
fv status
# Mode: whitelist
# Whitelisted:
#   - README.md (full)
#   - src/public_api.py (full)
#   - src/core.py#1-50
# Everything else: veiled

# Agent can only see these specific files/sections
# All other files show as ...
```

### Scenario 3: Combined Mode (Whitelist + Blacklist Exceptions)

```bash
# Start with whitelist mode - minimal visibility
fv init --mode whitelist
fv unveil src/public_api.py

# But we also want to hide specific implementation details
# even within the whitelisted file
fv veil src/public_api.py#80-120

# Status:
# Mode: whitelist
# Whitelisted:
#   - src/public_api.py (full, except lines 80-120)
# Everything else: veiled
```

### Scenario 4: Agent Focused Development

```bash
# Start with minimal visibility
fv init --mode whitelist
fv unveil README.md

# As agent asks questions, gradually whitelist more
fv unveil src/core.py#1-50

# Agent needs to see implementation?
fv unveil src/core.py#51-100

# Continue whitelisting sections as needed
# Agent never sees what you haven't explicitly whitelisted
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

# Restore veil state for next session
fv restore
```

### Scenario 6: Safe Experimentation

```bash
# Save current working state
fv checkpoint save "working"

# Try different configuration
fv mode whitelist
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

#### Blacklist Mode
```yaml
version: 1
mode: blacklist
blacklist:
  - secrets.env                 # Hide entire file
  - config/production.yaml      # Hide entire file
  - api.py#10-20                # Hide lines 10-20
  - api.py#50-60                # Hide lines 50-60
  - src/internal/               # Hide entire directory
  - utils.py#5-15,30-40         # Hide multiple ranges
```

#### Whitelist Mode
```yaml
version: 1
mode: whitelist
whitelist:
  - README.md                   # Show entire file
  - src/public_api.py           # Show entire file
  - core.py#1-50                # Show lines 1-50
  - utils.py#1-20,100-150       # Show multiple ranges

# In whitelist mode, everything not listed is veiled
# You can also add blacklist exceptions on top:
blacklist:
  - src/public_api.py#80-120    # Hide these lines even in whitelisted file
```

### Path Format Specification

**Full file or directory:**
```
secrets.env
src/internal/
```

**Partial file (line ranges):**
```
file.py#10-20          # Single range
file.py#10-20,30-40    # Multiple ranges
file.py#10-20,30-40,50-60
```

**Rules:**
- `#` separates path from line ranges
- `-` separates start and end line (inclusive)
- `,` separates multiple ranges
- Line numbers are 1-indexed
- Ranges must not overlap
- Directories cannot have line ranges (must end with `/`)

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

**Blacklist Mode:**
```python
def is_veiled(file, line):
    for entry in config.blacklist:
        path, ranges = parse_entry(entry)
        if file_matches(file, path):
            if ranges is None:  # Full file or directory
                return True
            for start, end in ranges:
                if start <= line <= end:
                    return True
    return False
```

**Whitelist Mode:**
```python
def is_veiled(file, line):
    # First check if blacklisted (exception to whitelist)
    for entry in config.blacklist:
        path, ranges = parse_entry(entry)
        if file_matches(file, path):
            if ranges is None:
                return True
            for start, end in ranges:
                if start <= line <= end:
                    return True
    
    # Then check if whitelisted
    for entry in config.whitelist:
        path, ranges = parse_entry(entry)
        if file_matches(file, path):
            if ranges is None:  # Full file
                return False
            for start, end in ranges:
                if start <= line <= end:
                    return False
    
    # Default: veiled in whitelist mode
    return True
```

### Entry Parsing

```python
def parse_entry(entry: str) -> (str, Optional[List[Tuple[int, int]]]):
    """
    Parse config entry into path and line ranges.
    
    Examples:
        "secrets.env" -> ("secrets.env", None)
        "api.py#10-20" -> ("api.py", [(10, 20)])
        "api.py#10-20,30-40" -> ("api.py", [(10, 20), (30, 40)])
        "src/internal/" -> ("src/internal/", None)
    """
    if '#' in entry:
        path, ranges_str = entry.rsplit('#', 1)
        ranges = []
        for range_str in ranges_str.split(','):
            start, end = map(int, range_str.split('-'))
            ranges.append((start, end))
        return path, ranges
    return entry, None
```

### File Matching

```python
def file_matches(file: str, pattern: str) -> bool:
    """
    Check if file matches pattern.
    
    Examples:
        file_matches("src/api.py", "src/api.py") -> True
        file_matches("src/api.py", "src/") -> True
        file_matches("src/api.py", "api.py") -> False
    """
    if pattern.endswith('/'):  # Directory
        return file.startswith(pattern)
    return file == pattern
```

### Algorithms

**Veil Algorithm:**
1. Read original file
2. Determine veiled lines based on mode and config
3. Extract hidden content
4. Save to `.funveil/objects/{filename}.{start}-{end}`
5. Replace in file:
   - First line: `...\n`
   - Middle lines: `\n`
   - Last line: `...\n`
6. Set read-only: `chmod 444`
7. Update config

**Unveil Algorithm:**
1. Read object file(s)
2. Restore content to working file
3. Delete object file(s)
4. If no veils remain: `chmod 644`
5. Update config (remove entry)

**Apply Mode Algorithm:**
1. Scan all files in project (respecting .gitignore)
2. For each file, determine veiled lines based on mode
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
| Invalid line range | Show valid format: `file.py#10-20,30-40` |
| Directory with line ranges | Error: directories cannot have line ranges |

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
- ✅ Compact config format with inline line ranges

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

1. **Initialize** once per project (blacklist or whitelist mode)
2. **Configure** visibility (blacklist specific items, or whitelist specific items)
3. **Work** with agent in veiled directory
4. **Unveil all** before committing real changes
5. **Restore** configuration after commit
6. **Checkpoint** important states for safety

The dual-mode design supports both:
- **Blacklist mode**: Hide secrets while keeping most code visible
- **Whitelist mode**: Limit agent to minimal visible subset

The compact config format `file.py#10-20,30-40` makes it easy to specify exactly what should be visible or hidden.
