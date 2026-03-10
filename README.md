 # Funveil

[![License](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](https://github.com/e6qu/funveil)
[![Build](https://github.com/e6qu/funveil/workflows/CI/badge.svg)](https://github.com/e6qu/funveil/actions)
[![Coverage](https://img.shields.io/badge/Coverage-90.68%25-brightgreen)](https://github.com/e6qu/funveil/actions)
[![Coverage Gate](https://img.shields.io/badge/Coverage%20Gate-80%25-brightgreen)](https://github.com/e6qu/funveil/actions/workflows/ci.yml)
[![Diff Coverage](https://img.shields.io/badge/Diff%20Coverage-no%20regression-brightgreen)](https://github.com/e6qu/funveil/actions/workflows/ci.yml)
[![Lines of Code](https://img.shields.io/badge/LOC-16%2C620-blue)](https://github.com/e6qu/funveil)
[![Test Count](https://img.shields.io/badge/Tests-478-green)](https://github.com/e6qu/funveil)

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
