# Funveil Specification

> A lightweight tool for controlling file visibility in AI agent workspaces.

## Overview

Funveil creates a "veiled" view of a codebase where specific files or line
ranges can be hidden from an AI agent. Fully-veiled files are physically
removed from disk; partially-veiled files retain visible lines with markers.
Hidden content is stored in a content-addressable store with parsed symbol
metadata and can be restored at any time.

```
~/project/
├── .funveil_config        # Configuration (protected)
├── .funveil/
│   ├── objects/           # Content-addressable storage
│   ├── metadata/          # Parsed symbol metadata (parallel to objects/)
│   │   └── index.json     # Consolidated symbol index
│   ├── checkpoints/       # Saved veil states
│   ├── profiles/          # Named veil configurations
│   ├── history/           # Undo/redo action history
│   ├── analysis/          # Parser cache
│   └── manifest.json      # Current disclosure state snapshot
├── api.py                 # File with veiled sections → ...[a3f7d2e]...
└── secrets.env            # Fully veiled file → removed from disk
```

## Specification Index

| Spec | Covers |
|------|--------|
| [specs/commands.md](specs/commands.md) | CLI commands and flags |
| [specs/config.md](specs/config.md) | `.funveil_config` format, gitignore management |
| [specs/patterns.md](specs/patterns.md) | Path, regex, and line range syntax |
| [specs/veil-format.md](specs/veil-format.md) | Physical removal, veil markers, write protection, binary handling |
| [specs/storage.md](specs/storage.md) | CAS, metadata, checkpoints, history, analysis cache, GC |
| [specs/algorithms.md](specs/algorithms.md) | Veil resolution, core operations, edge cases |
| [specs/patch.md](specs/patch.md) | Patch parsing format |

## Other Documentation

| Doc | Covers |
|-----|--------|
| [README.md](README.md) | Quick start |
| [docs/TUTORIAL.md](docs/TUTORIAL.md) | User guide for LLM agents |
| [docs/LANGUAGE_FEATURES.md](docs/LANGUAGE_FEATURES.md) | 12-language parsing, entrypoints, call graphs |
| [docs/DESIGN_INTELLIGENT_VEILING.md](docs/DESIGN_INTELLIGENT_VEILING.md) | Architecture and design decisions |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Development setup, testing, pre-commit hooks |
| [MUTATION_TESTING.md](MUTATION_TESTING.md) | Mutation testing guide and results |

## Design Principles

- **Simple**: Single directory, normal git, no kernel modules
- **Safe**: Write protection, checkpoints for recovery
- **Transparent**: Clear veil state, easy to inspect
- **Compatible**: Works with standard Unix tools

## Security

- Objects stored as plain text (not encrypted)
- Write protection uses Unix permissions (owner can bypass)
- `.funveil/` and `.funveil_config` should be in `.gitignore`
- Not a security boundary — protection against accidents
- Symlinks validated to prevent escaping project root

## Trade-offs

**What you get**: simple implementation, no special privileges, works
everywhere (Linux, macOS, containers), content-addressable dedup, hash-based
verification, regex patterns, checkpoints.

**What you give up**: manual `unveil --all` / `restore` for git commits,
missing files in commits if you forget to unveil, ~2x disk for veiled
content, no fine-grained access control.

## License

AGPL-3.0
