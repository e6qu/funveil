# Funveil

A lightweight tool for controlling file visibility in AI agent workspaces.

## Overview

Funveil creates a "veiled" view of a codebase where specific files or line ranges can be hidden from an AI agent while preserving file structure and line numbers. This allows:

- **Focused agent work**: Show only relevant parts of a codebase
- **Secret protection**: Hide API keys, credentials, sensitive configuration
- **Gradual revelation**: Unveil sections as the agent needs them
- **Safety**: Prevent accidental modification of hidden content

## Building

Requires Rust 1.70+:

```bash
# Clone the repository
git clone https://github.com/yourusername/funveil.git
cd funveil

# Build release binary
cargo build --release

# The binary will be at target/release/fv
./target/release/fv --help
```

### Development Build

```bash
# Quick debug build for development
cargo build

# Run tests
cargo test

# Run full CI checks
make ci
```

## Quick Start

```bash
# Initialize (whitelist mode by default)
fv init

# Unveil specific files for the agent
fv unveil README.md
fv unveil src/public_api.py

# Check status
fv status

# Before committing, unveil everything
fv unveil --all
git add .
git commit -m "Changes"

# Restore veils for next session
fv restore
```

## Two Configuration Modes

### Whitelist Mode (Default)
Everything hidden, whitelist specific items. Use this to limit the agent to a minimal subset.

```yaml
mode: whitelist
whitelist:
  - README.md
  - src/public_api.py
  - core.py#1-50
```

### Blacklist Mode
Everything visible, blacklist specific items. Use this to hide secrets while keeping most code visible.

```yaml
mode: blacklist
blacklist:
  - config/secrets.env
  - '/.*\.env$/'
  - api.py#80-120
```

## Veil Types

**Full Veil**: Entire file hidden (`...`)

**Partial Veil**: Specific line ranges hidden with hash markers:
```python
# api.py - lines 4-6 veiled
def public():
    ...[a3f7d2e]  # First line shows hash
                # Middle lines blank
    ...           # Last line
    visible_code()
```

## Architecture

- **Strong types**: `LineRange`, `ContentHash`, `Pattern` are newtypes with validation
- **Content-addressable storage**: SHA-256 with 3-level prefix (`objects/xx/yy/<hash>`)
- **Deduplication**: Identical content stored once
- **Veil markers**: Show truncated hash: `...[a3f7d2e]...`
- **Permission preservation**: Original Unix permissions restored on unveil

## Documentation

| Document | Description |
|----------|-------------|
| [docs/TUTORIAL.md](docs/TUTORIAL.md) | **Start here** - Practical guide for LLM coding agents |
| [SPEC.md](SPEC.md) | Complete specification with all commands, modes, and formats |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Development setup, testing, and contribution guidelines |
| [LANGUAGE_SUPPORT_PLAN.md](LANGUAGE_SUPPORT_PLAN.md) | Supported languages for intelligent veiling |
| [docs/DESIGN_INTELLIGENT_VEILING.md](docs/DESIGN_INTELLIGENT_VEILING.md) | Architecture for code-aware veiling |

## Key Commands

```bash
fv init [--mode blacklist|whitelist]   # Initialize funveil
fv status                              # Show current veil state
fv veil <pattern>                      # Hide file/directory/pattern
fv unveil <pattern>                    # Reveal file/directory/pattern
fv unveil --all                        # Reveal everything (for git commits)
fv restore                             # Restore previous veil state
fv apply                               # Re-apply veils after editing
fv checkpoint save <name>              # Save veil state
fv checkpoint restore <name>           # Restore saved state
fv doctor                              # Check integrity
fv gc                                  # Garbage collect unused objects
```

## Language Support

Funveil supports code-aware veiling for multiple languages:

- **Rust**, **Go**, **Zig** - Systems programming
- **Python**, **TypeScript**, **Bash** - Application scripting
- **HTML**, **CSS** - Web frontend
- **XML**, **Markdown** - Data and documentation

See [LANGUAGE_SUPPORT_PLAN.md](LANGUAGE_SUPPORT_PLAN.md) for details.

## License

AGPL-3.0
