# Funveil

A lightweight tool for controlling file visibility in AI agent workspaces.

## Overview

Funveil creates a "veiled" view of a codebase where specific files or line ranges can be hidden from an AI agent while preserving the file structure and line numbers.

## Building

Requires Rust 1.70+:

```bash
cargo build --release
```

The binary will be at `target/release/fv`.

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

# Restore veils
fv restore
```

## Configuration

Configuration is stored in `.funveil_config`:

```yaml
version: 1
mode: whitelist
whitelist:
  - README.md
  - src/public.py
  - core.py#1-50
blacklist:
  - src/public.py#80-120  # Exceptions within whitelisted files
```

## Architecture

- **Strong types**: `LineRange`, `ContentHash`, `Pattern` are newtypes with validation
- **Content-addressable storage**: SHA-256 with 3-level prefix (`objects/xx/yy/<hash>`)
- **Deduplication**: Identical content stored once
- **Veil markers**: Show truncated hash: `...[a3f7d2e]...`
- **Permission preservation**: Original Unix permissions restored on unveil

## License

AGPL-3.0
