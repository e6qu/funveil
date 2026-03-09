 # Funveil

A lightweight tool for controlling file visibility in AI agent workspaces.

## Overview

Funveil creates a "veiled" view of a codebase where specific files or line ranges can be hidden from an AI agent while preserving file structure and line numbers.

## Key Features

- **Two modes**: Whitelist (show only what's needed) or Blacklist (hide secrets)
- **Partial veils**: Hide specific line ranges within files
- **Content-addressable storage**: Hidden content stored with SHA-256 hash
- **Permission preservation**: Original Unix permissions restored on unveil
- **Checkpoints**: Save and restore veil states

## Quick Start

```bash
# Initialize (default: whitelist mode)
fv init

# Unveil what the agent needs
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

## Building

```bash
cargo build --release
```

## Documentation

- **[docs/TUTORIAL.md](docs/TUTORIAL.md) - Start here
- **[SPEC.md](SPEC.md) - Complete specification
- **[CONTRIBUTING.md](CONTRIBUTING.md) - Development setup

## License

AGPL-3.0
