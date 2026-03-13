# Funveil

[![License](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](https://github.com/e6qu/funveil)
[![Build](https://github.com/e6qu/funveil/workflows/CI/badge.svg)](https://github.com/e6qu/funveil/actions)
<!-- badge:coverage -->
[![Coverage](https://img.shields.io/badge/Coverage-97.00%25-brightgreen)](https://github.com/e6qu/funveil)
<!-- badge:tests -->
[![Tests](https://img.shields.io/badge/Tests-1239-green)](https://github.com/e6qu/funveil)
<!-- badge:loc -->
[![Code LOC](https://img.shields.io/badge/Code%20LOC-10%2C344-blue)](https://github.com/e6qu/funveil)
<!-- badge:test-loc -->
[![Test LOC](https://img.shields.io/badge/Test%20LOC-24%2C643-blue)](https://github.com/e6qu/funveil)

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
- **[SPEC.md](SPEC.md)** - Specification index
- **[specs/](specs/)** - Detailed specs (config, storage, veil format, CLI, algorithms)
- **[docs/LANGUAGE_FEATURES.md](docs/LANGUAGE_FEATURES.md)** - Supported languages & analysis
- **[CONTRIBUTING.md](CONTRIBUTING.md)** - Development setup
- **[MUTATION_TESTING.md](MUTATION_TESTING.md)** - Mutation testing guide & results

## License

AGPL-3.0
