# Coverage Workflow

This project enforces **line** and **branch** coverage via `cargo-llvm-cov` on nightly. CI has two gates:

| Gate | Line floor | Branch floor | Regression tolerance |
|------|-----------|-------------|---------------------|
| Absolute (every build) | 96% | 87% | n/a |
| Diff (PRs only) | 96% | 87% | 1% relative to base |

## Quick reference

```bash
# 1. Generate JSON + human-readable report
cargo +nightly llvm-cov --all-features --branch --json --output-path /tmp/cov.json
cargo +nightly llvm-cov report --all-features --branch 2>&1 | tee /tmp/cov.txt

# 2. See totals
python3 -c "
import json
d = json.load(open('/tmp/cov.json'))
t = d['data'][0]['totals']
print(f\"Line:   {t['lines']['percent']:.2f}%\")
print(f\"Branch: {t['branches']['percent']:.2f}%\")
"

# 3. Find uncovered lines (sorted by most missed)
cargo +nightly llvm-cov --all-features --branch --show-missing-lines 2>&1 | tee /tmp/cov-missing.txt
python3 coverage_report.py --text /tmp/cov-missing.txt

# 4. Filter to a single file
python3 coverage_report.py --text /tmp/cov-missing.txt --file history.rs

# 5. LCOV mode (pure line-level, useful for editors)
cargo +nightly llvm-cov --all-features --branch --lcov --output-path /tmp/cov.lcov
python3 coverage_report.py --lcov /tmp/cov.lcov --file cas.rs
```

## Step-by-step: finding and fixing uncovered branches

### Step 1: Generate the report

```bash
cargo +nightly llvm-cov --all-features --branch --show-missing-lines 2>&1 | tee /tmp/cov-missing.txt
```

This produces a table like:

```
Filename                      Regions    Missed ... Lines   Missed  Cover   Branches  Missed  Cover   Missing Lines
src/history.rs                 120        4           280      6    97.86%    48        8    83.33%   283,285,299,301,312,314
```

### Step 2: Identify which lines are yours

Cross-reference the "Missing Lines" column with your diff:

```bash
git diff main -- src/history.rs | grep '^+' | head -30
```

Or use `coverage_report.py` for a cleaner view:

```bash
python3 coverage_report.py --text /tmp/cov-missing.txt --file history.rs
```

Output:

```
--- history.rs  (-6 lines / 280) ---
  L283, L285, L299, L301, L312, L314
  branches: L283, L299, L312
```

### Step 3: Read the source at those lines

Open the file and check what each uncovered line does. Typical causes:

| Pattern | Example | How to test |
|---------|---------|-------------|
| Error branch in `?` | `fs::write(&tmp, content)?` | Make the path unwritable with `chmod` |
| `if err` guard | `if entries.is_empty()` | Construct the edge-case input directly |
| `Err(_) => false` | `match hash.path_components()` | Pass an intentionally invalid value |
| `tracing::warn!` | Warning on range clip | Trigger the condition; the line still counts |
| Cleanup/rollback | `cleanup_temps(&staged)` | Force a failure after staging temp files |

### Step 4: Write targeted tests

Each uncovered branch needs a test that **forces execution through that path**. Common techniques:

**Filesystem errors (permission denied, disk full):**

```rust
#[cfg(unix)]
#[test]
fn test_restore_fails_on_unwritable_dir() {
    use std::os::unix::fs::PermissionsExt;
    // ... create dir, chmod 0o444, attempt write, assert error ...
    // cleanup: chmod 0o755
}
```

**Invalid/edge-case inputs:**

```rust
#[test]
fn test_path_components_short_hash() {
    let hash = ContentHash { 0: "abc".to_string() }; // bypass from_string validation
    assert!(hash.path_components().is_err());
}
```

If you can't construct the value directly (private fields), test through the public API that reaches the branch:

```rust
#[test]
fn test_exists_returns_false_on_invalid_hash() {
    // ContentHash::from_string rejects < 7 chars, so exists() Err branch
    // is unreachable through public API. If it's dead code, consider
    // removing the branch or making the field pub(crate) for testing.
}
```

### Step 5: Verify the fix

Re-run coverage on just the file you changed:

```bash
cargo +nightly llvm-cov --all-features --branch --show-missing-lines 2>&1 \
  | grep -A1 'history.rs'
```

Or re-run the full report and compare totals to confirm the branch number went up.

## Understanding the metrics

**Line coverage** ("lines" in llvm-cov JSON) is actually **region-based**. A single source line with `let x = foo()?;` counts as two regions: the success path and the `?` error path. This is stricter than pure line coverage.

**Branch coverage** counts conditional branches: `if`, `match` arms, `&&`/`||` short-circuit, `?` operator. Each branch has a true/false (or multi-arm) path and both must execute.

This means you can have a line "covered" but still have uncovered branches on it (e.g., `?` where the error path never fires).

## CI behavior

- **`coverage` job** (every push): runs `cargo +nightly llvm-cov --all-features --branch`, checks absolute floors (96% line, 87% branch). Fails the build if below.
- **`diff-coverage` job** (PRs only): runs coverage on both base and PR branch, compares. Fails if either floor is violated OR if coverage drops more than 1% relative to base. Posts a comment on the PR with per-file breakdown.

## Files

| File | Purpose |
|------|---------|
| `coverage_report.py` | CLI tool to parse llvm-cov text or LCOV output, shows uncovered lines/branches per file |
| `.github/workflows/ci.yml` | CI coverage jobs (`coverage`, `diff-coverage`) |
| `Makefile` (`update-badges`) | Generates coverage JSON and updates README badges |
