# `DirectorySource::read_file` returns misleading error on macOS for missing files

## Context

While fixing `DirectorySource::file_exists` (commit `75a66aa`) I
identified that the old `read_file` implementation has the same
macOS-specific bug on its missing-file branch that `file_exists`
now avoids.

### The bug

`DirectorySource::read_file` at `src-tauri/src/wasm/source.rs:152-192`:

```rust
let canonical_root = self.root.canonicalize()?;

let canonical = match full_path.canonicalize() {
    Ok(p) => p,
    Err(_) => {
        // Missing file branch.
        let normalized = normalize_path(&full_path);
        anyhow::ensure!(
            normalized.starts_with(&canonical_root),
            "plugin file path `{path}` escapes the plugin directory"
        );
        return std::fs::read(&full_path)
            .with_context(|| format!("reading plugin file `{path}`"));
    }
};
```

On macOS, `tempdir` paths are under `/var/folders/...` but the
canonicalized form is `/private/var/folders/...`. When the file
doesn't exist, `full_path` is built lexically (`/var/...`) and the
non-canonical `normalize_path` on it still yields `/var/...`. The
`starts_with` check against `canonical_root` (`/private/var/...`)
then fails even though the file is legitimately inside the plugin
root.

Result: a missing-file read surfaces as the wrong error message —
`"plugin file path \`foo\` escapes the plugin directory"` instead
of the expected `"reading plugin file \`foo\`: No such file or
directory"`.

### Why it hasn't been caught

The existing test `reject_nonexistent_file`
(`src-tauri/src/wasm/source.rs:773`) asserts only `result.is_err()`.
Both branches of the buggy behavior produce `Err`, so the test
passes silently.

### How `file_exists` avoids it

`file_exists` (added in the same commit `75a66aa`) skips the
canonicalization-based root check on the missing-file branch
entirely, relying on the upstream `validate_plugin_path` guard for
the lexical-path rejection. See the comment block at
`src-tauri/src/wasm/source.rs:200-213`.

## What to do

Apply the same fix to `read_file`. The `Err(_)` branch should just
delegate to `std::fs::read(&full_path)` directly (whose own error
includes a correct "not found" diagnostic), dropping the
`normalized.starts_with(canonical_root)` check — `validate_plugin_path`
has already enforced the lexical bound before any filesystem work.

Also tighten the `reject_nonexistent_file` test to assert the error
message is about the missing file, not about path escape — otherwise
we'll drift back into this hole.

## Acceptance

- `read_file` on a missing file inside a macOS tempdir produces an
  error whose message names the file / OS error, not "escapes the
  plugin directory."
- `reject_nonexistent_file` asserts a precise error message.
- The symlink-escape case (existing file that canonicalizes outside
  root) still errors with the "escapes" message — that logic lives
  in the `Ok(canonical)` branch and is unaffected.

## Related

- Commit `75a66aa` — `wasm/source: add PluginSource::file_exists`
  (fixed the same bug for the new method).
- Code review on 2026-04-20 flagged this as a "Note" (pre-existing,
  not a regression).
