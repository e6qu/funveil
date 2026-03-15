# Funveil Roadmap

## 1. Physical File Removal for Fully-Veiled Files

**Current behavior:** When a file is fully veiled, its content is stored in CAS and the file on disk is replaced with a 4-byte marker (`...\n`). The file remains visible in the filesystem.

**Proposed behavior:** Fully-veiled files should be physically removed from disk after their content is stored in CAS. This better reflects the intent of veiling (hiding files from AI agents) since agents that scan the filesystem won't see the file at all — not even a marker.

### Design

- Add a `removed: bool` field to `ObjectMeta` (defaults to `true` for new veils, `false` for backward compat)
- `veil_file` for full files: after storing in CAS, `fs::remove_file` instead of writing marker
- `unveil_file`: restore from CAS regardless of whether the file exists on disk
- `show` command: when file doesn't exist on disk but is in config, display `[REMOVED — stored in CAS]`
- `status --files`: report removed files as `state: "veiled", on_disk: false`
- `apply`: handle missing-on-disk files gracefully (skip re-veil if already removed)
- `doctor`: check that every veiled file with `removed: true` has a valid CAS entry

### Migration

Existing veiled files (with marker) continue to work. A `fv apply` could optionally upgrade them to removed state.

---

## 2. Parser-Based Veiling and Unveiling

Funveil already has a tree-sitter parser supporting 12 languages (Rust, Go, TypeScript, Python, Bash, Terraform/HCL, YAML, Zig, HTML, CSS, XML, Markdown) and a `HeaderStrategy` that shows only function/class signatures while hiding implementations. This foundation enables much richer veiling strategies.

### Ideas

**Selective symbol veiling.** Instead of veiling entire files or fixed line ranges, let users veil specific symbols by name:

```
fv veil src/auth.rs --symbol check_password
fv veil src/auth.rs --symbol "impl AuthService"
```

The parser identifies the symbol's line range and veils just that. Unveiling works the same way. This is more ergonomic than manually specifying `#L10-25`.

**Call-graph-aware veiling.** Use the existing `CallGraphBuilder` and `EntrypointDetector` to automatically determine what to veil:

```
fv veil --unreachable-from main    # veil everything not reachable from main()
fv unveil --callers-of process     # unveil process() and everything that calls it
```

This leverages the existing `TraceDirection::Forward` and `TraceDirection::Backward` tracing to make disclosure decisions based on program structure rather than file paths.

**Layered disclosure strategy.** Define disclosure levels that progressively reveal more:

| Level | What's visible | Use case |
|-------|---------------|----------|
| 0 | File list only | Initial orientation |
| 1 | Signatures + docstrings (HeaderStrategy) | Understanding API surface |
| 2 | Signatures + bodies of called functions | Following a specific flow |
| 3 | Full source | Deep work on a specific area |

```
fv veil src/ --level 1              # show headers only
fv unveil src/auth.rs --level 2     # reveal bodies of called functions
```

**Smart partial unveiling.** When an AI agent requests context about a function, automatically unveil its direct dependencies (imports, called functions) but keep the rest veiled. This could be driven by a `fv context <function>` command that uses the call graph to determine the minimum set of code needed.

**Language-aware diff veiling.** When veiling partial ranges, align to syntax boundaries (function, class, block) rather than raw line numbers. The parser already extracts `body_range` for each symbol, so alignment is straightforward.

---

## 3. Progressive Disclosure for Context Window Optimization

Funveil's core objective is to serve as a progressive disclosure tool that optimizes how AI agents use their context windows. A typical codebase has far more code than fits in a context window, but most tasks only require a fraction of it. Progressive disclosure means revealing code incrementally — just enough for the agent to make progress — rather than dumping everything upfront.

### The Problem

AI agents working on codebases face a fundamental tension:

- **Too little context:** The agent hallucinates APIs, misses constraints, produces incompatible code
- **Too much context:** The window fills with irrelevant code, displacing information the agent actually needs, increasing cost, and degrading quality as attention dilutes across thousands of lines

Current approaches (`.gitignore`-style exclusion, repo maps, file summaries) are static — they don't adapt to what the agent is actually doing.

### Funveil's Approach

Funveil sits between the codebase and the AI agent, controlling what the agent can see at any point:

1. **Start veiled.** Begin with the entire codebase veiled. The agent sees file names and module structure but no implementation details.

2. **Header-first disclosure.** Unveil headers (signatures, type definitions, public APIs) across the codebase. This gives the agent the API surface — enough to understand interfaces and plan work — in a fraction of the tokens.

3. **Targeted deep disclosure.** As the agent identifies which functions/modules it needs to modify or understand, selectively unveil those. The call graph guides what related code should come along.

4. **Automatic re-veiling.** Once the agent finishes with a section of code, re-veil it to free up context window space for the next task.

### Why This Works

- **Token efficiency.** A function signature is ~10-50 tokens. Its implementation might be 200-2000. Showing headers first gives 10-40x more coverage per token.
- **CAS deduplication.** Storing content at multiple disclosure levels is cheap because identical content shares the same CAS hash. Switching between levels is just pointer swaps.
- **Checkpoint integration.** The existing checkpoint system can snapshot disclosure state, allowing agents to save/restore their "view" of the codebase.
- **Undo/redo support.** The new action history system means disclosure changes are reversible. An agent can speculatively unveil code, determine it's not needed, and undo.

### Future Directions

**Token budget mode.** Accept a token budget and automatically select the optimal set of code to disclose:

```
fv disclose --budget 50000 --focus src/auth/
```

This would use the call graph to prioritize code reachable from the focus area, disclosing at the appropriate level (headers vs full) to maximize information density within the budget.

**Agent integration protocol.** A structured JSON interface where agents can request disclosure:

```json
{"action": "disclose", "target": "src/auth.rs::verify_token", "depth": 2}
```

The `--json` output mode already provides machine-readable responses. Extending this to accept structured disclosure requests would make funveil a first-class tool in AI agent toolchains.

**Semantic chunking.** Rather than file-level or line-level granularity, chunk code by semantic units (a function and its helper functions, a struct and its impl block, a module's public API). The parser already extracts these boundaries.
