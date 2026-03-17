# Funveil

[![License](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](https://github.com/e6qu/funveil)
[![Build](https://github.com/e6qu/funveil/workflows/CI/badge.svg)](https://github.com/e6qu/funveil/actions)
[![Line Coverage](https://img.shields.io/badge/Line%20Coverage-97.40%25-brightgreen)](https://github.com/e6qu/funveil) <!-- badge:coverage -->
[![Branch Coverage](https://img.shields.io/badge/Branch%20Coverage-89.39%25-brightgreen)](https://github.com/e6qu/funveil) <!-- badge:branch-coverage -->
[![Tests](https://img.shields.io/badge/Tests-1255-green)](https://github.com/e6qu/funveil) <!-- badge:tests -->
[![Code LOC](https://img.shields.io/badge/Code%20LOC-14%2C040-blue)](https://github.com/e6qu/funveil) <!-- badge:loc -->
[![Test LOC](https://img.shields.io/badge/Test%20LOC-40%2C263-blue)](https://github.com/e6qu/funveil) <!-- badge:test-loc -->

A lightweight tool for controlling file visibility in AI agent workspaces.

## Overview

Funveil creates a "veiled" view of a codebase where specific files or line ranges can be hidden from an AI agent while preserving file structure and line numbers.

## Key Features

- **Two modes**: Whitelist (show only what's needed) or Blacklist (hide secrets)
- **Physical file removal**: Fully-veiled files are removed from disk, not replaced with markers
- **Partial veils**: Hide specific line ranges within files
- **Content-addressable storage**: Hidden content stored with SHA-256 hash
- **Metadata system**: Parsed symbol metadata stored alongside CAS for blind queries ([details](specs/storage.md#metadata))
- **Query-based unveiling**: Unveil by symbol name, callers, or callees (`--symbol`, `--callers-of`, `--callees-of`)
- **Layered disclosure**: 4 levels from metadata-only to full source (`--level 0-3`)
- **Token budget mode**: Automatic disclosure planning within a token budget (`fv disclose --budget`)
- **Smart context**: Unveil a function and its dependencies (`fv context <fn> --depth N`)
- **Permission preservation**: Original Unix permissions restored on unveil
- **Undo/redo**: Reversible veil operations with full action history
- **Checkpoints**: Save and restore veil states
- **12-language parsing**: Code-aware veiling with tree-sitter ([details](docs/LANGUAGE_FEATURES.md))

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

- **[docs/TUTORIAL.md](docs/TUTORIAL.md)** - Start here
- **[SPEC.md](SPEC.md)** - Specification index
- **[specs/](specs/)** - Detailed specs (config, storage, veil format, CLI, algorithms)
- **[docs/LANGUAGE_FEATURES.md](docs/LANGUAGE_FEATURES.md)** - Supported languages & analysis
- **[CONTRIBUTING.md](CONTRIBUTING.md)** - Development setup
- **[MUTATION_TESTING.md](MUTATION_TESTING.md)** - Mutation testing guide & results
- **[Running under gVisor](docs/GVISOR.md)** — OS-level sandboxing with runsc

## License

AGPL-3.0
