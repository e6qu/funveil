# Funveil Specification

> A lightweight tool for controlling file visibility in AI agent workspaces.

**Documentation**: [README.md](README.md) | [TUTORIAL.md](docs/TUTORIAL.md) | [CONTRIBUTING.md](CONTRIBUTING.md) | [LANGUAGE_SUPPORT_PLAN.md](LANGUAGE_SUPPORT_PLAN.md)

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
├── .funveil_config             # Funveil configuration (not veilable)
├── .funveil/                   # Funveil data directory
│   ├── objects/                # Hidden content storage (CAS)
│   └── checkpoints/            # Saved veil states
├── api.py                      # File with veiled sections
└── secrets.env                 # Fully veiled file
```

**Veil operation:**
1. Extract hidden lines from file
2. Compute content hash, store in `.funveil/objects/xx/yy/<hash>`
3. Replace in file with markers showing hash: `...[a3f7d2e]...`
4. Set read-only (original permissions preserved in index)

**Unveil operation:**
1. Look up hash in object index
2. Retrieve content from CAS
3. Restore content to working file
4. Restore original permissions
5. Update config

## Core Concepts

### Two Configuration Modes

Funveil supports two complementary approaches to visibility control. Patterns are applied in the order they appear in the config file.

#### Mode 1: Blacklist
**Everything visible, blacklist specific items**

Use this when you want to hide specific secrets or implementation details while keeping most of the codebase visible.

```yaml
mode: blacklist
blacklist:
  - /.*\.env$/                  # Hide all .env files (regex) - applied first
  - config/production.env       # This file (already veiled by regex above)
  - api.py#10-20                # Hide lines 10-20 only
  - src/internal/               # Hide entire directory
  - /test_.*\.py$/              # Hide test files (regex)
```

#### Mode 2: Whitelist (Default)
**Everything hidden, whitelist specific items**

Use this when you want to limit the agent to a minimal subset of the codebase.

```yaml
mode: whitelist
whitelist:
  - README.md                   # Show entire file
  - src/public_api.py           # Show entire file
  - core.py#1-50                # Show lines 1-50 only
  - utils.py#1-20,100-150       # Show multiple ranges
  - /src\/public\/.*/           # Show all files in src/public/ (regex)
```

You can also combine both: start with whitelist mode, then apply a blacklist on top.

### Veil Types

**Full Veil**: Entire file hidden
```
secrets.env content: API_KEY=abc123
Veiled display: ...
```

**Partial Veil**: Specific line ranges hidden (text files only)

The veiled content is replaced with markers showing the content hash:

```python
# api.py - lines 4-6 veiled (3 lines)
1: import os
2: 
3: def public():
4:     ...[a3f7d2e]
5: 
6:     ...
7:     return result

# api.py - lines 4-5 veiled (2 lines)
1: import os
2: 
3: def public():
4:     ...[a3f7d2e]
5:     ...
6:     visible_code()

# api.py - line 4 veiled (1 line)
1: import os
2: 
3: def public():
4:     ...[a3f7d2e]...
5:     visible_code()
```

**Marker format:**
- First line of veiled range: `...[hash]` (first 7 chars of SHA-256)
- Middle lines (if any): blank (line preserved but empty)
- Last line (if different from first): `...`
- Single line: `...[hash]...` (combined)

The hash allows verification and retrieval of the original content from the object store.

**Why 7 characters?** Same as git short hash - provides 16^7 (~268 million) combinations, sufficient for uniqueness while keeping markers readable.

**Binary Files**: Can only be veiled in full (no line ranges)
- Binary files are detected by content analysis or file extension
- Attempting to veil specific line ranges in a binary file raises an error
- When veiled, binary files display as `...` (same as full veil)

### Line Preservation

Veiled files maintain the same line count:
- Hidden lines replaced with markers showing content hash
- Format: `...[hash]` on first line, `...` on last line (if different)
- Single line veiled: `...[hash]...`
- Middle lines replaced with blank lines
- Line numbers preserved for visible content
- File size changes (markers vs original content)

### Default Exclusions

The following version control directories are automatically excluded from all veiling operations:
- `.git/`, `.svn/`, `.hg/`, `.cvs/`, `bzr/`, `.fslckout/`
- `_FOSSIL_`, `_darcs/`, `CVS/`

Additionally, the funveil directories themselves are protected:
- `.funveil/` - Object storage directory
- `.funveil_config` - Configuration file

These directories/files are never veiled, unveiled, or shown in status. They are treated as if they don't exist.

### Write Protection and Permission Preservation

Files with any veiled content are set read-only (`chmod 444`):
- Prevents accidental modification
- Must unveil before editing
- Works with all editors (standard Unix permissions)

**Original permissions are preserved:**
- When veiling, original permissions stored in object index
- When unveiling, original permissions restored (not just 644)
- If file had specific ownership/group, it is preserved

## Commands

### Core Operations

```bash
fv init [--mode blacklist|whitelist]
    Initialize funveil in current directory
    Creates .funveil/ structure and .funveil_config
    Default mode: whitelist

fv status
    Show current veil state
    # Blacklist mode output:
    # Mode: blacklist
    # Blacklisted (5):
    #   - secrets.env (full)
    #   - config/production.yaml (full)
    #   - api.py#10-20,50-60
    #   - /.*\.env$/ (regex)
    #   - /test_.*\.py$/ (regex)
    # 
    # Whitelist mode output:
    # Mode: whitelist
    # Whitelisted (3):
    #   - README.md (full)
    #   - src/public_api.py (full)
    #   - core.py#1-50
    # Everything else: veiled
    # 
    # New files auto-veiled (2):
    #   - config/staging.env (matched by /.*\.env$/)

fv mode [blacklist|whitelist]
    Show or change configuration mode
    # Show current mode:
    #   fv mode
    # Switch to whitelist mode:
    #   fv mode whitelist
    # Switch to blacklist mode:
    #   fv mode blacklist

fv veil <pattern>[#<start>-<end>[,<start>-<end>]]
    Hide file, directory, or line range
    # In blacklist mode: adds to blacklist
    # In whitelist mode: adds to blacklist (exception)
    # Examples:
    #   fv veil secrets.env
    #   fv veil api.py#10-20
    #   fv veil api.py#10-20,50-60
    #   fv veil src/internal/
    #   fv veil '/.*\.env$/'
    #   fv veil '/test_.*\.py$/#10-20'

fv unveil <pattern>[#<start>-<end>[,<start>-<end>]]
    Reveal hidden content
    # In blacklist mode: removes from blacklist
    # In whitelist mode: adds to whitelist
    # Examples:
    #   fv unveil secrets.env
    #   fv unveil api.py#10-20
    #   fv unveil --all

fv apply
    Re-apply veils to all files
    # Use after editing files that have veiled sections
    # Ensures veils are correctly applied to modified content
    # New files matching patterns are auto-veiled with warning

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

### Maintenance

```bash
fv doctor
    Check veil integrity
    # Verifies:
    # - All object hashes exist in CAS
    # - Config is valid
    # - No orphaned objects
    # - File permissions are correct

fv gc
    Garbage collect unused objects
    # Removes objects from CAS that are no longer referenced
    # Use periodically to reclaim disk space
    # Example: fv gc
    #          # Removes 12 unused objects, freed 45KB

fv clean
    Remove all funveil data (unveil first!)
```

## Use Scenarios

### Scenario 1: Hiding Secrets (Blacklist Mode)

```bash
# Clone repository with sensitive config
git clone https://github.com/company/api.git
cd api

# Initialize in blacklist mode
fv init --mode blacklist

# Blacklist production secrets
fv veil config/production.env
fv veil .env

# Blacklist all .env files everywhere
fv veil '/.*\.env$/'

# Check status
fv status
# Mode: blacklist
# Blacklisted:
#   - config/production.env (full)
#   - .env (full)
#   - /.*\.env$/ (regex)

# Agent sees only ...
cat config/production.env
...

# File is protected
chmod config/production.env
# -r--r--r-- (read-only)
```

### Scenario 2: Minimal Visibility (Whitelist Mode)

```bash
# Initialize in whitelist mode (default)
fv init

# Whitelist only what agent needs
fv unveil README.md
fv unveil src/public_api.py
fv unveil src/core.py#1-50

# Whitelist all files in public directory
fv unveil '/src\/public\/.*/'

# Check status
fv status
# Mode: whitelist
# Whitelisted:
#   - README.md (full)
#   - src/public_api.py (full)
#   - src/core.py#1-50
#   - /src\/public\/.*/ (regex)
# Everything else: veiled

# Agent can only see these specific files/sections
# All other files show as ...
```

### Scenario 3: Combined Mode (Whitelist + Blacklist Exceptions)

```bash
# Start with whitelist mode - minimal visibility
fv init
fv unveil src/public_api.py

# But we also want to hide specific implementation details
# even within the whitelisted file
fv veil src/public_api.py#80-120

# Also hide all test files (regex)
fv veil '/test_.*\.py$/'

# Status:
# Mode: whitelist
# Whitelisted:
#   - src/public_api.py (full, except lines 80-120)
# Blacklisted (exceptions):
#   - /test_.*\.py$/ (regex)
# Everything else: veiled
```

### Scenario 4: Agent Focused Development

```bash
# Start with minimal visibility
fv init
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

### Scenario 6: Editing Veiled Files

```bash
# You need to unveil a file to edit it
fv unveil api.py#10-20

# Edit the file
vim api.py

# Re-apply veils after editing
fv apply
# Warning: api.py has been modified, veils re-applied
```

### Scenario 7: New Files Auto-Veiled

```bash
# You're in whitelist mode
# Create a new file
echo "API_KEY=secret" > new_config.env

# Funveil detects new files matching patterns
fv status
# New files auto-veiled (1):
#   - new_config.env (matched by /.*\.env$/)

# The file is automatically veiled on next apply/status
# You can unveil it if the agent needs to see it
fv unveil new_config.env
```

### Scenario 8: Deleted Files

```bash
# A veiled file is deleted
rm secrets.env

# Funveil detects this
fv status
# Warning: veiled file 'secrets.env' not found
# The veil remains in config but file is missing

# To clean up:
fv unveil secrets.env  # Removes veil from config
# or
rm .funveil_config && fv init  # Start fresh
```

### Scenario 9: Safe Experimentation

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

### Scenario 10: Recovery

```bash
# Something went wrong
fv doctor
# Error: Object file missing

# Restore from checkpoint
fv checkpoint restore "morning-backup"

# Or restore to clean state
fv unveil --all
rm -rf .funveil/ .funveil_config
fv init
```

### Scenario 11: Garbage Collection

```bash
# After many veil/unveil cycles, clean up unused objects
fv gc
# Removed 23 unused objects, freed 128KB

# Check integrity after GC
fv doctor
# All checks passed
```

## Data Formats

### Config: `.funveil_config`

Located in project root. **This file is protected and cannot be veiled.**

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
  - /.*\.env$/                  # Hide all .env files (regex)
  - /test_.*\.py$/              # Hide test files (regex)
  - /.*\.secret$/               # Hide all .secret files (regex)
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
  - /src\/public\/.*/           # Show all in src/public/ (regex)

# In whitelist mode, everything not listed is veiled
# You can also add blacklist exceptions on top:
blacklist:
  - src/public_api.py#80-120    # Hide these lines even in whitelisted file
  - /.*_test\.py$/              # Hide all test files (regex)
```

### Path/Pattern Format Specification

**Important:** All paths are resolved relative to project root. Symlinks are followed but must resolve within the project directory.

**Literal path (full file or directory):**
```
secrets.env
src/internal/
```

**Literal path with line ranges:**
```
file.py#10-20          # Single range
file.py#10-20,30-40    # Multiple ranges
file.py#10-20,30-40,50-60
```

**Regex pattern (wrapped in `/`):**
```
/.*\.env$/             # All .env files
/test_.*\.py$/         # Test files
/src\/public\/.*/      # All files in src/public/
```

**Regex with line ranges:**
```
/.*\.env$/#10-20              # Lines 10-20 in all .env files
/test_.*\.py$/#1-10,50-60     # Multiple ranges in test files
```

**Rules:**
- All paths must be relative to project root (local paths only)
- Paths starting with `.` or `..` are **errors** (no relative paths)
- Hidden files must use full path: `path/.env` not `.env`
- `#` separates path/pattern from line ranges
- `-` separates start and end line (inclusive)
- `,` separates multiple ranges
- Line numbers are 1-indexed (first line is 1, not 0)
- Start must be <= end (10-5 is invalid)
- Ranges must not overlap
- Line numbers beyond file length are clamped to file length
- Binary files can only be veiled in full (no line ranges)
- Directories must end with `/` and cannot have line ranges
- Regex patterns wrapped in `/` like JavaScript
- Regex patterns are matched against full relative path

### Path Validation

**Valid paths:**
```
README.md               # File in root
src/api.py              # File in subdirectory
src/internal/           # Directory
/.*\.env$/              # Regex pattern
```

**Invalid paths (raise error):**
```
./README.md             # ERROR: relative path
../config.yaml          # ERROR: relative path
.env                    # ERROR: starts with dot, use path/.env
```

### Regex Guidelines

Keep regex patterns simple and deterministic:

**Good patterns:**
```
/.*\.env$/              # All .env files
/test_.*\.py$/          # Test files
/src\/.*\/public\.py$/  # public.py in any src subdirectory
```

**Avoid:**
- Complex backreferences
- Lookaheads/lookbehinds (if performance matters)
- Patterns that could match infinitely
- Escaped special characters when not needed

### Objects: `.funveil/objects/` (Content-Addressable Storage)

Files are stored by content hash (SHA-256) with 3-level prefix for distribution:

```
.funveil/objects/
├── a3/
│   └── 5f/
│       └── 7d2e9c...     # Content hash a35f7d2e9c...
├── b8/
│   └── 2e/
│       └── 9c4f1a...
```

**Benefits:**
- **Deduplication**: Identical content stored once regardless of source file
- **Integrity**: Hash verification detects corruption
- **Distribution**: 3-level prefix prevents too many files in one directory

**Storage format:**
- Raw file content (no compression initially)
- SHA-256 hash of content = storage path

**Object index** (stored in config):
```yaml
objects:
  "api.py#10-20":
    hash: a35f7d2e9c...b8
    permissions: "644"       # Original permissions preserved
    owner: "1000:1000"       # Original owner:group
  "secrets.env":
    hash: b82e9c4f1a...d2
    permissions: "600"
    owner: "1000:1000"
```

### Checkpoints: `.funveil/checkpoints/{name}/`

Complete snapshot using content-addressable storage:

```
checkpoints/stable/
├── manifest.yaml       # File list with content hashes
├── metadata.json       # Created timestamp, stats
└── refs/               # References to objects (optional optimization)
```

**manifest.yaml structure:**
```yaml
created: 2024-03-06T12:00:00Z
mode: whitelist
files:
  README.md:
    hash: a35f7d2e9c...b8   # Content hash
    lines: null              # Full file visible
    permissions: "644"
    owner: "1000:1000"
  src/api.py:
    hash: b82e9c4f1a...d2   # Content hash
    lines: [[1, 50]]         # Only lines 1-50 visible
    permissions: "644"
    owner: "1000:1000"
  secrets.env:
    hash: c93f8e5b2c...e4   # Content hash
    lines: []                # Fully veiled
    permissions: "600"
    owner: "1000:1000"
```

**Deduplication across checkpoints:**
- All checkpoints reference the same object store
- Identical files between checkpoints stored once
- Manifests are small (just hashes and metadata)

## Implementation Summary

### Determining What to Veil

**Blacklist Mode:**
```python
def is_veiled(file, line):
    for entry in config.blacklist:
        pattern, ranges = parse_entry(entry)
        if match_pattern(file, pattern):
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
        pattern, ranges = parse_entry(entry)
        if match_pattern(file, pattern):
            if ranges is None:
                return True
            for start, end in ranges:
                if start <= line <= end:
                    return True
    
    # Then check if whitelisted
    for entry in config.whitelist:
        pattern, ranges = parse_entry(entry)
        if match_pattern(file, pattern):
            if ranges is None:  # Full file
                return False
            for start, end in ranges:
                if start <= line <= end:
                    return False
    
    # Default: veiled in whitelist mode
    return True
```

### Auto-Veil New Files

When running `fv apply` or `fv status`:
1. Scan all files in project (respecting .gitignore)
2. Identify files matching patterns but not yet veiled/unveiled
3. Auto-veil new files in whitelist mode
4. Show warning: "New files auto-veiled: file1, file2"
5. Add entries to config

### Entry Parsing

```python
def parse_entry(entry: str) -> (Pattern, Optional[List[Tuple[int, int]]]):
    """
    Parse config entry into pattern and line ranges.
    
    Examples:
        "secrets.env" -> (Literal("secrets.env"), None)
        "api.py#10-20" -> (Literal("api.py"), [(10, 20)])
        "/.*\\.env$/" -> (Regex(".*\\.env$"), None)
        "/test_.*\\.py$/#10-20" -> (Regex("test_.*\\.py$"), [(10, 20)])
        "src/internal/" -> (Literal("src/internal/"), None)
    
    Raises:
        ValueError: If path starts with . or ..
    """
    # Check for relative path
    if entry.startswith('./') or entry.startswith('../'):
        raise ValueError(f"Relative paths not allowed: {entry}")
    if entry.startswith('.') and not entry.startswith('/'):
        raise ValueError(f"Hidden files must use full path: {entry}")
    
    # Separate pattern from line ranges
    if '#' in entry and not entry.startswith('/'):
        # Literal path with ranges
        path, ranges_str = entry.rsplit('#', 1)
        ranges = parse_ranges(ranges_str)
        return LiteralPattern(path), ranges
    elif '#' in entry and entry.startswith('/'):
        # Regex with ranges - find # after closing /
        pattern_end = entry.rfind('/', 1)  # Find last /
        if '#' in entry[pattern_end:]:
            pattern_str, ranges_str = entry.rsplit('#', 1)
            ranges = parse_ranges(ranges_str)
            return RegexPattern(pattern_str[1:-1]), ranges
    
    # No ranges
    if entry.startswith('/') and entry.endswith('/'):
        return RegexPattern(entry[1:-1]), None
    return LiteralPattern(entry), None

def parse_ranges(ranges_str: str) -> List[Tuple[int, int]]:
    """Parse '10-20,30-40' into [(10, 20), (30, 40)]"""
    ranges = []
    for range_str in ranges_str.split(','):
        start, end = map(int, range_str.split('-'))
        ranges.append((start, end))
    return ranges
```

### Pattern Matching

```python
@dataclass
class LiteralPattern:
    path: str
    
    def matches(self, file: str) -> bool:
        if self.path.endswith('/'):  # Directory
            return file.startswith(self.path)
        return file == self.path

@dataclass
class RegexPattern:
    pattern: str
    compiled: re.Pattern = field(init=False)
    
    def __post_init__(self):
        self.compiled = re.compile(self.pattern)
    
    def matches(self, file: str) -> bool:
        return bool(self.compiled.match(file))

def match_pattern(file: str, pattern: Pattern) -> bool:
    return pattern.matches(file)
```

### Symlink Handling

**Rules:**
1. Symlinks are followed during file operations
2. Symlinks pointing outside the project root raise an error
3. Symlinks pointing to parent directories raise an error
4. This prevents veiling files outside the project through symlinks

```python
def resolve_path(path: str, root: Path) -> Path:
    """
    Resolve path, following symlinks.
    Raises error if resolved path is outside project root.
    """
    full_path = (root / path).resolve()
    
    # Check if resolved path is within project root
    try:
        full_path.relative_to(root.resolve())
    except ValueError:
        raise ValueError(
            f"Path '{path}' resolves outside project root: {full_path}"
        )
    
    return full_path

def is_safe_symlink(path: str, root: Path) -> bool:
    """
    Check if path is a symlink pointing within project root.
    """
    full_path = root / path
    
    if not full_path.is_symlink():
        return True  # Not a symlink, always safe
    
    # Follow the symlink and check where it points
    target = full_path.resolve()
    root_resolved = root.resolve()
    
    # Check if target is within project root
    try:
        target.relative_to(root_resolved)
        return True
    except ValueError:
        return False  # Points outside project root
```

**Security considerations:**
- Prevents `fv veil ../../etc/passwd` via symlink
- Prevents symlink chains that escape the project
- Error message clearly indicates the issue

### Algorithms

**Veil Algorithm:**
1. Read original file
2. Determine veiled lines based on mode and config
3. Extract hidden content
4. Compute SHA-256 hash of extracted content
5. Store in CAS: `.funveil/objects/xx/yy/<hash>` (3-level prefix)
6. Update object index: map `filename#start-end` → hash + permissions
7. Replace in file with markers showing hash:
   - Single line veiled: `...[first7chars]...\n`
   - First of multiple lines: `...[first7chars]\n`
   - Middle lines: `\n` (blank)
   - Last of multiple lines: `...\n`
8. Set read-only: `chmod 444`
9. Update config

**Unveil Algorithm:**
1. Look up hash from object index for this file/range
2. Retrieve content from CAS: `.funveil/objects/xx/yy/<hash>`
3. Restore content to working file
4. Restore original permissions from object index
5. Remove entry from object index
6. If no veils remain: file is writable
7. Update config (remove entry)

**Note:** Content is not deleted from CAS immediately (garbage collection can be run separately)

**Empty file handling:**
- Empty files (0 bytes) cannot be partially veiled (error: "cannot veil empty file")
- Empty files can be fully veiled (display as `...`)

**Duplicate entries:**
- Duplicate patterns in config are allowed but redundant
- Last occurrence wins for overlapping ranges in same file

**Line ending handling:**
- Markers preserve original file's line endings (CRLF or LF)
- When veiling CRLF file, markers use CRLF
- When unveiling, original line endings are restored from CAS

**Timestamps:**
- File modification times (mtime) are NOT preserved during veil/unveil
- This is intentional - veiled file is "new" content with markers
- If you need original timestamps, use checkpoints

**Hard links:**
- Hard links are followed to the actual file
- Veiling a hard-linked file only affects that path
- Other paths to the same inode remain unchanged
- This is filesystem-standard behavior

**Extended attributes:**
- Extended attributes (xattr) are NOT preserved
- Only standard Unix permissions (mode/owner/group) are stored

**Concurrent access:**
- No file locking implemented (simplification)
- Last operation wins if multiple instances run simultaneously
- Use checkpoints for safety when unsure
- Doctor command can detect inconsistencies

**Atomicity:**
- Operations are best-effort atomic
- If interrupted, run `fv doctor` to check integrity
- Restore from checkpoint if corruption detected
- Config is written before files are modified (so safe to retry)

**Apply Mode Algorithm:**
1. Scan all files in project (respecting .gitignore and default VCS exclusions)
2. For each file, determine veiled lines based on mode:
   - Blacklist mode: veil only blacklisted files/lines
   - Whitelist mode: veil all except whitelisted files/lines
3. Patterns are applied in config file order (first match wins for files, last range wins for overlapping lines)
4. Extract and store veiled content
5. Replace with markers in working files
6. Auto-veil new files matching patterns (with warning)

**Checkpoint Save:**
1. For each file in project, compute SHA-256 hash of content
2. Store content in CAS if not already present: `.funveil/objects/xx/yy/<hash>`
3. Write manifest with file paths, hashes, permissions, and veil state
4. Write metadata.json with timestamp and stats

**Checkpoint Restore:**
1. Auto-save current as `auto-before-restore`
2. Read checkpoint manifest
3. For each file in manifest, retrieve content from CAS by hash
4. Restore working files (overwrites with checkpoint content)
5. Restore original permissions
6. Re-apply veil state from manifest

**Garbage Collection Algorithm:**
1. Scan all objects in `.funveil/objects/`
2. Build set of all referenced hashes from:
   - Current config object index
   - All checkpoint manifests
3. Delete unreferenced objects
4. Report freed space

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
| Binary file with line ranges | Error: binary files can only be veiled in full |
| Relative path | Error: use full path from project root |
| Hidden file without path | Error: use `path/.env` not `.env` |
| Invalid regex | Show regex syntax error |
| Symlink outside project | Error: symlink points outside project root |
| Attempt to veil VCS dir | Error: VCS directories are excluded by default |
| .funveil_config cannot be veiled | Error: config file is protected |
| .funveil/ cannot be veiled | Error: funveil data directory is protected |

### Implementation Details

**Language:** Python 3.8+ or Go 1.18+

**Dependencies:**
- YAML parsing (PyYAML or Go yaml package)
- Regex support (built-in)
- Gitignore parsing (optional, for file scanning)
- Standard library only for core functionality

**Estimated Size:** ~900-1200 lines of code

**Testing:**
- Unit tests for each command
- Integration tests for workflows
- Property tests for line preservation
- Edge cases: empty files, binary files, unicode, line endings (CRLF/LF)
- Mode switching tests
- Regex pattern tests
- Path validation tests
- Symlink security tests
- GC and checkpoint tests
- Concurrent access tests (best effort)

## Trade-offs

### What We Get

- ✅ Simple implementation (~900-1200 lines)
- ✅ No kernel modules or special privileges
- ✅ Works everywhere (Linux, macOS, containers)
- ✅ All core features (veil/unveil, line preservation, checkpoints)
- ✅ Standard Unix permissions for write protection
- ✅ Two complementary modes (blacklist and whitelist)
- ✅ Compact config format with inline line ranges
- ✅ Regex support for pattern matching
- ✅ Content-addressable storage with deduplication
- ✅ Hash in veil markers for verification
- ✅ Strict path validation (no relative paths)
- ✅ Secure symlink handling
- ✅ Automatic garbage collection

### What We Give Up

- ⚠️ Manual `unveil --all` / `restore` for git commits
- ⚠️ Git commits contain `...[hash]...` if you forget to unveil
- ⚠️ ~2x disk usage for veiled content (original + CAS)
- ⚠️ No fine-grained access control (just read-only/not)

## Security Notes

- Objects stored as plain text (not encrypted)
- Same filesystem permissions as working files
- `.funveil/` and `.funveil_config` should be in `.gitignore` (not committed)
- Write protection uses Unix permissions (can be bypassed by owner)
- Not a security boundary, just protection against accidents
- Symlinks are validated to prevent escaping project root

## License

AGPL-3.0 (see LICENSE file)

## Summary

Funveil provides a simple, effective way to control what an AI agent can see in a codebase:

1. **Initialize** once per project (whitelist or blacklist mode)
2. **Configure** visibility using literal paths or regex patterns
3. **Work** with agent in veiled directory
4. **Apply** veils after editing files (`fv apply`)
5. **Unveil all** before committing real changes
6. **Restore** configuration after commit
7. **Checkpoint** important states for safety
8. **GC** periodically to clean up unused objects

The dual-mode design supports both:
- **Whitelist mode**: Limit agent to minimal visible subset (default)
- **Blacklist mode**: Hide secrets while keeping most code visible

The compact config format `file.py#10-20` or `/.*\.env$/` makes it easy to specify exactly what should be visible or hidden.

The veil markers `...[a3f7d2e]...` show the content hash for easy verification and retrieval.
