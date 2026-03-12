# Funveil

[![License](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](https://github.com/e6qu/funveil)
[![Build](https://github.com/e6qu/funveil/workflows/CI/badge.svg)](https://github.com/e6qu/funveil/actions)
[![codecov](https://codecov.io/gh/e6qu/funveil/graph/badge.svg)](https://codecov.io/gh/e6qu/funveil)
[![Mutation Testing](https://img.shields.io/badge/Mutation%20Testing-85%25-yellow)](MUTATION_TESTING.md)
<!-- badge:loc -->[![Lines of Code](https://img.shields.io/badge/LOC-24%2C592-blue)](https://github.com/e6qu/funveil)
<!-- badge:tests -->[![Test Count](https://img.shields.io/badge/Tests-1227-green)](https://github.com/e6qu/funveil)

A lightweight tool for controlling file visibility in AI agent workspaces.

## Overview

Funveil creates a "veiled" view of a codebase where specific files or line ranges can be hidden from an AI agent while preserving file structure and line numbers.

## Key Features

- **Two modes**: Whitelist (show only what's needed) or Blacklist (hide secrets)
- **Partial veils**: Hide specific line ranges within files
- **Content-addressable storage**: Hidden content stored with SHA-256 hash
- **Permission preservation**: Original Unix permissions restored on unveil
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
- **[SPEC.md](SPEC.md)** - Complete specification
- **[docs/LANGUAGE_FEATURES.md](docs/LANGUAGE_FEATURES.md)** - Supported languages & analysis
- **[CONTRIBUTING.md](CONTRIBUTING.md)** - Development setup
- **[MUTATION_TESTING.md](MUTATION_TESTING.md)** - Mutation testing guide & results

## License

AGPL-3.0
