# Funveil Tutorial for LLM Coding Agents

A practical guide for using Funveil to control workspace visibility.

---

## Quick Start (30 seconds)

```bash
# 1. Build the tool
cargo build --release

# 2. Initialize in your project
cd /path/to/project
~/funveil/target/release/fv init

# 3. Whitelist files the agent needs
fv unveil README.md src/core.py

# 4. Work with the agent...

# 5. Before commit, unveil all
fv unveil --all
git commit -m "changes"
fv restore  # Restore veil state
```

---

## Common Prompts

### "Show me only the public API"

```bash
# Whitelist mode - hide everything by default
fv init --mode whitelist

# Unveil only public interfaces
fv unveil src/public_api.py
fv unveil src/types.py#1-50  # Just the first 50 lines (imports + types)

# Check what's visible
fv status
```

### "Hide all secrets and tests"

```bash
# Blacklist mode - show everything by default
fv init --mode blacklist

# Veil secrets
fv veil '/.*\.env$/'
fv veil config/secrets.yaml

# Veil tests (optional)
fv veil '/.*_test\.py$/'
fv veil '/tests\/.*/'
```

### "I'm working on a specific function"

```bash
# Parse code structure
fv parse src/calculator.py

# Veil everything except this function's dependencies
fv trace-backward calculate_sum --depth 2
# Shows what calls calculate_sum

# Unveil the call chain
fv unveil src/calculator.py
fv unveil src/validators.py
```

### "Start minimal, reveal gradually"

```bash
# Start with minimal visibility
fv init
fv unveil README.md

# As agent asks questions, gradually reveal
fv unveil src/auth.py#1-30      # Show imports and class signature
fv unveil src/auth.py#31-100    # Later: show implementation
fv unveil src/utils.py          # Eventually: show helpers
```

---

## Pattern Syntax Cheat Sheet

| Pattern | Meaning |
|---------|---------|
| `file.py` | Entire file |
| `file.py#10-20` | Lines 10-20 only |
| `file.py#1-10,50-60` | Lines 1-10 and 50-60 |
| `src/` | Entire directory |
| `/.*\.env$/` | Regex: all .env files |
| `/test_.*\.py$/` | Regex: all test files |

---

## Workflow Patterns

### Pattern 1: Secure by Default

```bash
# New project - start locked down
fv init

# Agent only sees README
# As they ask questions, whitelist specific files:
fv unveil src/main.py#1-50      # Entry point
fv unveil src/config.py         # Configuration
```

### Pattern 2: API-First Development

```bash
# Show only public API, hide implementations
fv init
fv unveil src/api.py            # Public routes
fv unveil src/types.py          # Type definitions

# Hide internal implementation
fv veil '/.*_internal\.py$/'
fv veil src/api.py#200-500      # Hide large handler implementations
```

### Pattern 3: Task-Focused Sessions

```bash
# Working on authentication?
fv init

# Unveil only auth-related files
fv unveil src/auth/
fv unveil src/middleware/auth.py

# Use --all before commit when done
fv unveil --all && git commit -m "Update auth" && fv restore
```

---

## Code-Aware Veiling

### Header Mode (Show signatures only)

```bash
# Parse and show only function signatures
fv parse --format summary src/main.py

# Output shows:
# - Functions with their line ranges
# - Classes and methods
# - Imports
```

### Trace Call Graphs

```bash
# Forward: what does this function call?
fv trace-forward process_payment --depth 2

# Backward: what calls this function?
fv trace-backward validate_token --depth 3

# In tree format
fv trace-forward process_payment --format tree
```

### Find Entrypoints

```bash
# Show all entrypoints
fv entrypoints

# Filter by type
fv entrypoints --type main       # Main functions
fv entrypoints --type test       # Test functions
fv entrypoints --type handler    # HTTP handlers
```

---

## Safety Commands

```bash
# Check integrity
fv doctor

# Save checkpoint before big changes
fv checkpoint save "before-refactor"

# Restore if something goes wrong
fv checkpoint restore "before-refactor"

# Clean up unused objects
fv gc
```

---

## Git Integration

Always unveil before committing:

```bash
# Pre-commit workflow
fv unveil --all          # Show all files
git diff                 # Review actual changes
git add .                # Stage
git commit -m "..."      # Commit
fv restore               # Restore veil state
```

Or use a git alias:

```bash
# Add to ~/.gitconfig
[alias]
    fvc = !fv unveil --all && git commit
    fvp = !fv unveil --all && git push
```

---

## Configuration Examples

### `.funveil_config` (Whitelist Mode)

```yaml
version: 1
mode: whitelist
whitelist:
  - README.md
  - src/public_api.py
  - src/types.py#1-100
blacklist:
  - src/public_api.py#500-600  # Hide implementation details
```

### `.funveil_config` (Blacklist Mode)

```yaml
version: 1
mode: blacklist
blacklist:
  - '/.*\.env$/'
  - config/secrets.yaml
  - '/.*_test\.py$/'
```

---

## Troubleshooting

| Problem | Solution |
|---------|----------|
| "Cannot edit file" | Unveil first: `fv unveil file.py` |
| Wrong line numbers | Ranges are 1-indexed, not 0-indexed |
| Pattern not matching | Use full paths from project root |
| Hidden file error | Use `path/.env` not `.env` |
| Forgot to unveil before commit | `git reset HEAD~1`, unveil, recommit |

---

## Language-Specific Tips

### Rust
```bash
# Show public API only
fv unveil src/lib.rs
fv veil '/.*_test\.rs$/'
fv veil '/tests\/.*/'
```

### Python
```bash
# Hide __pycache__ and venv (automatic)
# Veil secrets
fv veil '/.*\.env$/'
fv veil 'config/production.yaml'
```

### TypeScript/React
```bash
# Show component interfaces
fv unveil src/components/
# Veil generated code
fv veil '/.*\.generated\.(ts|tsx)$$/'
```

### Go
```bash
# Hide test files
fv veil '/.*_test\.go$/'
# Show main package entrypoints
fv entrypoints --type main
```

---

## Further Reading

- [SPEC.md](../SPEC.md) - Complete specification
- [CONTRIBUTING.md](../CONTRIBUTING.md) - Development setup
- [LANGUAGE_SUPPORT_PLAN.md](../LANGUAGE_SUPPORT_PLAN.md) - Supported languages
