# fv tool — observations (v0.4.0)

## Bugs

### `fv trace` — positional `FUNCTION` argument is confusing / unusable

The help text says `[FUNCTION]` is "Function name to start tracing from (use with --from)",
implying the positional argument is only meaningful *together* with `--from`. That makes
`--from` redundant and the surface area confusing. Worse, `--from` and `--from-entrypoint`
are mutually exclusive, so if you pass `--from-entrypoint` you cannot also restrict to a
starting function — but the error message just says `cannot be used with` without explaining
what to do instead.

**Reproduce:**

```
fv trace --from main --from-entrypoint   # error, no actionable hint
```

### `fv disclose` — shows a *plan* but never shows code

`fv disclose --budget 8000 --focus <file>` prints a list of files and token counts, then
stops. The command is named "disclose code within a token budget" but never actually
outputs any code. There is no flag in `--help` to make it emit the file contents.

### `fv entrypoints` — classifies Markdown/shell/Terraform as entrypoints

Every `.md`, `.sh`, and `terraform/*.tf` file is listed as an entrypoint with roles like
`handler` or `main`. This inflates the entrypoint count (23 in this repo, most of which
are docs) and makes the output noisy for code-navigation purposes.

### `fv status` — output is nearly empty

`fv status` prints only `Mode: whitelist` with no indication of which files are veiled,
unveiled, or pending. Compare to `git status`, which shows staged/unstaged/untracked files.

### `fv trace --from-entrypoint` returns an unordered flat function list

With 182 entrypoints discovered, the output is a single merged list of all reachable
functions with no per-entrypoint grouping and no indication of which entrypoint reaches
which function. The `--format tree` flag is silently ignored when `--from-entrypoint` is
used.

## Missing features

### `fv disclose` — no `--show` / `--output` flag to emit actual code

There should be a way to go from the disclosure *plan* to the actual file contents within
the budget. Currently a user must take the plan and manually run `fv show` on each file.

### `fv trace` — no file-path targeting

There is no way to say "trace from every function defined in `main.py`". You must know a
specific function name. Combining `--focus <file>` (like `disclose`) with `trace` would
help navigate larger codebases.

### `fv disclose` — no multi-focus support

`--focus` accepts only a single file or function. Being able to pass multiple focus points
(e.g. `--focus a.py --focus b.py`) would allow budgeted disclosure across several related
modules.

### `fv status` — no file-level breakdown

There is no equivalent of `fv status --verbose` that lists veiled/unveiled files and their
current annotation state.

### `fv context` — undocumented / unclear purpose

`fv context --help` shows no examples and the description ("Show context around a
function") overlaps heavily with `fv show` and `fv trace`. A real-world usage example in
`--help` would clarify when to prefer it.

### Named veil profiles / sessions

Right now `fv veil` and `fv unveil` mutate a single global state. There is no way to
save a named configuration ("infrastructure-hidden", "tests-only") and switch between
them. A `fv profile save <name>` / `fv profile load <name>` workflow would let users
maintain reusable veil sets for different tasks (onboarding vs. debugging vs. code review).

### `fv veil --reachable-from <fn>` (inverse of `--unreachable-from`)

`--unreachable-from` hides everything *not* reachable from a function. The symmetric
flag — hide everything *except* what is reachable — would let you narrow the visible
surface to a single call-graph without having to enumerate files manually.

### Glob/pattern support in `fv veil` / `fv unveil`

`fv veil '**/*_test.py'` or `fv veil 'monite_gateway/infrastructure/**'` would make it
practical to veil large subtrees and file categories. Currently every file must be
specified individually, and directory arguments are rejected with an error.

## Design principle: progressive disclosure

Commands should default to the **lowest useful output** and require explicit flags to
reveal more. The pattern is:

```
fv <cmd>                  # one-line or structured summary
fv <cmd> --verbose        # add detail (descriptions, counts, locations)
fv <cmd> --expand <name>  # show the body of a specific item
fv <cmd> --all            # show everything
```

Auditing every command against this principle:

| command | default output | problem |
|---|---|---|
| `fv show` | full file, all lines | **too much** — should be outline by default |
| `fv parse --format summary` | 4 counts | OK as default |
| `fv parse --format detailed` | all signatures + all imports + all call-sites | **too much** — imports and calls should be opt-in |
| `fv trace` | flat list of all reachable function names | **too much** — should default to direct callees only (depth 1) |
| `fv trace --from-entrypoint` | merged list of 283 functions | **far too much** — should default to a per-entrypoint summary (name + direct callees) |
| `fv entrypoints` | every file including docs/shell/terraform | **too much** — should default to code-only; `--all` for other file types |
| `fv disclose` | a plan (file list + token counts) | **too little** — plan is useful but code should follow (see separate note) |
| `fv status` | `Mode: whitelist` | **too little** — should at minimum show a file count |
| `fv doctor` | `✓ All checks passed` | OK |
| `fv history` | empty (no recorded actions yet) | OK once there are entries |
| `fv cache status` | (not tested) | — |

The worst offenders are `fv show`, `fv trace`, and `fv parse --format detailed`.

## Veiling/unveiling mechanic — bugs and issues found

### `fv veil <dir>` does not recurse into directories

`fv veil monite_gateway/infrastructure/ --mode headers` exits with:

```
Error: Is a directory (os error 21)
```

Directory veiling is the primary use-case (e.g. "hide all infrastructure, show only
consumers"). Having to enumerate files individually defeats the purpose. `fv veil` should
accept a directory and apply the veil recursively to all matching files within it, the same
way `fv unveil` already supports an `--all` flag. This was also confirmed by the user.

### `fv veil --mode headers` (and `--level 1`) reports "FULLY VEILED" in `fv show`

After veiling a file with `--mode headers`, the actual file on disk is correctly
partially-veiled (function signatures retained, bodies collapsed to `{ ... N lines ... }`).
But `fv show` reports:

```
File: entity_users.py [FULLY VEILED]
Content is veiled. Use 'fv unveil ...' to view.
```

This is wrong — the headers *are* visible. `fv show` does not distinguish between full and
partial veils. It should render the actual on-disk content (signatures + collapsed bodies)
instead of refusing to show anything.

### Veil annotations use C-style `{ }` syntax in Python files

Collapsed bodies are rendered as:

```python
class OakNorthPrimaryRole {
    // ... 4 lines ...
}
def get_monite_role_id_from_oaknorth_roles(...) -> UUID: { ... 43 lines ... }
```

Python uses indentation, not braces; the `//` comment marker is JavaScript/C syntax. This
makes veiled Python files syntactically invalid and breaks any tool that tries to parse
them (linters, type-checkers, import machinery). The annotation format should either use
valid Python syntax or a clearly-inert marker:

```python
class OakNorthPrimaryRole:
    ...  # 4 lines hidden

def get_monite_role_id_from_oaknorth_roles(...) -> UUID:
    ...  # 43 lines hidden
```

### `fv unveil` adds a file to *both* the blacklist and whitelist

After `fv veil <file>` followed by `fv unveil <file>`, `fv status` shows:

```
Blacklisted:
  - monite_gateway/infrastructure/monite/entity_users.py
Whitelisted:
  - monite_gateway/infrastructure/monite/entity_users.py
```

The file should be removed from the blacklist entirely on unveil, not added to an opposing
list. The conflicting entries make the state ambiguous and hard to reason about.

### `fv undo` does not record veil/unveil operations

After `fv veil ... --symbol _create_pending_entity_users`, running `fv undo` gives:

```
Error: history is empty — nothing to undo
```

Veil and unveil operations are not added to the undo history, so mistakes cannot be
corrected without manually re-unveiling. All state-mutating commands should be undoable.

### `--unreachable-from` is on `fv veil` but not on `fv unveil`

`fv veil --unreachable-from <fn>` exists (hides everything not reachable from a function),
but there is no symmetric `fv unveil --unreachable-from` or `fv unveil --reachable-from`.
The flags are asymmetric across the two commands.

### `fv veil --symbol` veils the entire file, not just the symbol's lines

`fv veil <file> --symbol _create_pending_entity_users` (dry-run: "Would veil symbol …
lines 64-230") actually veils the *entire file*. `fv status` then shows the file in the
blacklist. The partial/symbol veil is not honoured.

## Usability issues

### `fv trace` help text is self-contradictory

```
Arguments:
  [FUNCTION]  Function name to start tracing from (use with --from)

Options:
  --from <FROM>  Function to trace from (shows what this function calls)
```

Both the positional argument and `--from` are described as "function to trace from". Only
one should exist.

### `fv disclose` budget errors are opaque

If the focus file + its dependencies exceed the budget, the tool silently truncates the
plan with no warning. A `--strict` flag that errors out, or at minimum a "budget exceeded,
X tokens dropped" notice, would help users tune their budget.

### `fv trace --no-std` still surfaces stdlib names like `str`, `len`, `bool`

With `--no-std` the output still contains Python builtins. These are not stdlib *modules*
so they pass the filter, but they add noise. A `--no-builtins` flag (or inclusion in
`--no-std`) would clean up output for Python projects.

### `fv show` is just `cat -n` — wrong level of abstraction, and three commands overlap

`fv show <file>` dumps the entire file with line numbers. For a large file this is as
noisy as opening it in an editor, and it ignores the whole point of veiling. There are
actually three commands that all do partial versions of the same job:

| command | what it gives you |
|---|---|
| `fv show` | full file, line-numbered |
| `fv parse --format summary` | counts only (3 functions, 1 class, …) |
| `fv parse --format detailed` | names + signatures + line ranges, no code |
| `fv context <fn>` | unclear (no example in `--help`) |

**Proposed redesign of `fv show`:** make it a *structured outline* by default, with
progressive-disclosure flags to expand individual pieces. Concretely:

```
fv show <file>                        # outline: module docstring + all signatures
fv show <file> --expand <fn>          # add the full body of one function/class
fv show <file> --expand '*'           # full file (current behaviour, opt-in)
fv show <file> --docstrings           # include docstrings in the outline
fv show <file> --imports              # include import block
```

Default output example:

```
# monite_gateway/consumers/in_life/entities/main.py
# 3 functions · 1 class · Python

class EntityCreationResult(NamedTuple):          # line 59
    monite_entity_id: UUID
    created_in_monite: bool

def _create_pending_entity_users(                # line 64
    oaknorth_application_id: UUID,
    monite_entity_id: UUID,
    entity_role_mapping: dict[str, str],
    monite_api_client: MoniteApiClient,
) -> None: ...

def _create_entity_and_roles_in_monite(          # line 233
    entity: Entity,
    monite_api_client: MoniteApiClient,
) -> EntityCreationResult: ...

def handler(event: EntityEvent) -> None: ...     # line 341
```

This is actually close to what `fv parse --format detailed` already computes internally —
it just needs to render code instead of metadata. Retiring `fv parse` as a user-facing
command (or demoting it to `fv debug parse`) and folding its logic into `fv show` would
clean up the CLI surface considerably.

`fv context` should either be merged into `fv show --expand <fn>` or given a clear
distinct purpose (e.g. "show callers + callees of a function as an annotated snippet").
