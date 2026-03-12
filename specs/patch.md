# Patch Format

Funveil can parse and apply patches in unified diff and git diff formats.
Parsed via a Pest PEG grammar (`src/patch/grammar.pest`).

## Supported Formats

**Unified diff:**
```
--- a/path/file.rs
+++ b/path/file.rs
@@ -10,5 +10,6 @@ fn example
 context
-deleted
+added
```

**Git diff** (extended headers):
```
diff --git a/path/file.rs b/path/file.rs
old mode 100644
new mode 100755
index abc1234..def5678 100644
--- a/path/file.rs
+++ b/path/file.rs
```

## Hunk Format

```
@@ -old_start,old_count +new_start,new_count @@ optional_section_name
```

Line prefixes: ` ` (context), `-` (deleted), `+` (added),
`\ No newline at end of file`.

## Patch Management

Patches have unique numeric IDs (starting from 1). Applied in FIFO order,
unapplied in LIFO order (only the latest patch can be unapplied).
