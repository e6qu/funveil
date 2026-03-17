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
# Unveil by symbol name (no need to know the file path)
fv unveil --symbol calculate_sum

# Or unveil a function and all its dependencies
fv context calculate_sum --depth 2

# Unveil everything that calls a function
fv unveil --callers-of calculate_sum

# Unveil everything a function calls
fv unveil --callees-of calculate_sum

# Parse code structure
fv parse src/calculator.py

# Trace what this function calls
fv trace --from calculate_sum --depth 2
```

### "Start minimal, reveal gradually"

```bash
# Start with minimal visibility
fv init
fv unveil README.md

# Show headers only (signatures, no implementations)
fv veil src/ --level 1

# As agent asks questions, reveal more
fv unveil --symbol process_payment --level 3    # Full source for focus area
fv unveil --callees-of process_payment          # Dependencies

# Or let the budget planner decide what to show
fv disclose --budget 50000 --focus src/auth/
```

---

## Pattern Syntax Cheat Sheet

| Pattern | Meaning |
|---------|---------|
| `file.py` | Entire file |
| `file.py#10-20` | Lines 10-20 only |
| `file.py#1-10,50-60` | Lines 1-10 and 50-60 |
| `src/` | Entire directory |
| `*.py` | Glob: all Python files |
| `src/**/*.rs` | Glob: Rust files under src/ (recursive) |
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

## Progressive Disclosure

Funveil supports layered disclosure levels (0–3) for fine-grained control over
how much code is visible. See [specs/commands.md](../specs/commands.md) for full
flag reference.

### Disclosure Levels

| Level | What's visible | Use case |
|-------|---------------|----------|
| 0 | Nothing (file removed from disk) | Initial orientation |
| 1 | Signatures + docstrings only | Understanding API surface |
| 2 | Signatures + called function bodies | Following a specific flow |
| 3 | Full source | Deep work on a specific area |

```bash
# Remove files from disk (default full veil)
fv veil src/ --level 0

# Show only signatures
fv veil src/ --level 1

# Full source for a specific file
fv unveil src/auth.rs --level 3
```

### Token Budget Mode

Let funveil decide the optimal disclosure within a token budget:

```bash
fv disclose --budget 50000 --focus src/auth/

# Preview the actual code that would be disclosed
fv disclose --budget 50000 --focus src/auth/ --show

# Multiple focus areas
fv disclose --budget 80000 --focus src/auth/ --focus src/middleware/

# Strict mode — error if budget is exceeded
fv disclose --budget 50000 --focus src/auth/ --strict
```

This outputs a disclosure plan: level 3 for focus files, level 2 for direct
dependencies, level 1 for remaining reachable code — all within the token
budget. See [specs/commands.md](../specs/commands.md#progressive-disclosure) for the full command reference.

### Symbol-Based Queries

```bash
# Unveil by symbol name
fv unveil --symbol verify_token

# Unveil everything that calls a function
fv unveil --callers-of verify_token

# Unveil everything a function calls
fv unveil --callees-of handle_request

# Veil everything not reachable from main
fv veil --unreachable-from main

# Unveil a function and its N-depth dependencies
fv context verify_token --depth 2
```

### Undo/Redo

All veil/unveil operations are reversible:

```bash
fv undo           # Reverse the last operation
fv redo           # Replay a previously undone operation
fv history        # Show action history
fv history --show 5   # Show details of action #5
```

---

## Code-Aware Veiling

> For full details on all 12 supported languages, see [LANGUAGE_FEATURES.md](LANGUAGE_FEATURES.md).

### Outline Mode (`fv show`)

```bash
# View file in outline mode (default — signatures only, bodies collapsed)
fv show src/main.py

# Expand a specific function
fv show src/main.py --expand process_payment

# Expand everything (full file)
fv show src/main.py --expand '*'

# Include imports and docstrings in outline
fv show src/main.py --imports --docstrings
```

### Header Mode (Show signatures only)

```bash
# Parse and show only function signatures
fv parse src/main.py --format summary

# Detailed format with imports and call sites
fv parse src/main.py --format detailed --imports --calls

# Output shows:
# - Functions with their line ranges
# - Classes and methods
# - Imports (with --imports)
# - Call sites (with --calls)
```

### Trace Call Graphs

```bash
# Forward: what does this function call?
fv trace --from process_payment --depth 2

# Backward: what calls this function?
fv trace --to validate_token --depth 3

# In tree format
fv trace --from process_payment --format tree

# Trace all functions in a file
fv trace --focus src/auth.rs

# From all entrypoints
fv trace --from-entrypoint --depth 2

# Exclude stdlib calls
fv trace --from process_payment --no-std
```

### Find Entrypoints

```bash
# Show code entrypoints (default — excludes non-code files and tests)
fv entrypoints

# Include non-code files (markdown, shell, terraform, etc.)
fv entrypoints --all

# Filter by type
fv entrypoints --entry-type main       # Main functions
fv entrypoints --entry-type test       # Test functions
fv entrypoints --entry-type handler    # HTTP handlers

# Filter by language
fv entrypoints --language python
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

# Save/load named profiles
fv profile save "api-review"     # Save current veil config as a profile
fv profile load "api-review"     # Restore a saved profile
fv profile list                  # List all saved profiles
fv profile delete "api-review"   # Delete a profile
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

> Funveil supports 12 languages with code-aware parsing. For the full reference, see [LANGUAGE_FEATURES.md](LANGUAGE_FEATURES.md).

### Rust

```bash
# Show public API only
fv unveil src/lib.rs
fv veil '/.*_test\.rs$/'
fv veil '/tests\/.*/'

# Find test entrypoints
fv entrypoints --entry-type test --language rust

# Parse file structure
fv parse src/lib.rs --format summary
```

### Python

```bash
# Hide __pycache__ and venv (automatic)
# Veil secrets
fv veil '/.*\.env$/'
fv veil 'config/production.yaml'

# Find handlers and CLI commands
fv entrypoints --entry-type handler --language python
fv entrypoints --entry-type cli --language python
```

### TypeScript/React

```bash
# Show component interfaces
fv unveil src/components/
# Veil generated code
fv veil '/.*\.generated\.(ts|tsx)$$/'

# Parse a React component
fv parse src/App.tsx --format summary

# Find test entrypoints
fv entrypoints --entry-type test --language type-script
```

### Go

```bash
# Hide test files
fv veil '/.*_test\.go$/'
# Show main package entrypoints
fv entrypoints --entry-type main --language go

# Trace from a function
fv trace --from HandleRequest --depth 2
```

---

## Further Reading

- [SPEC.md](../SPEC.md) - Specification index
- [specs/](../specs/) - Detailed specs (config, storage, veil format, etc.)
- [CONTRIBUTING.md](../CONTRIBUTING.md) - Development setup
- [LANGUAGE_FEATURES.md](LANGUAGE_FEATURES.md) - Supported languages & analysis features
