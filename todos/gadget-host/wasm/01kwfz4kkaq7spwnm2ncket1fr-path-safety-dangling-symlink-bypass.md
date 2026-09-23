# `canonical_under_root` accepts dangling symlinks that point outside the root

**Kind:** bug
**Severity:** high
**Area:** src-tauri/src/wasm/path_safety.rs

## Problem
`canonical_under_root` (src-tauri/src/wasm/path_safety.rs:98) walks up
the candidate to the deepest existing ancestor via
`split_at_existing_ancestor` (path_safety.rs:167), canonicalizes only
that ancestor, and re-attaches the remaining tail lexically:

```rust
while !existing.exists() {
    // strip file_name into tail, continue with parent
}
```

`Path::exists()` follows symlinks and returns `false` for a
**dangling** symlink. So for a candidate `/root/trapdoor` where
`trapdoor` is a symlink to `/outside/secret` and `/outside/secret`
does not exist (yet), the walk skips past the symlink: `trapdoor`
lands in the lexical tail, `/root` is canonicalized, and the resolved
path becomes `/root/trapdoor`, which passes the
`resolved.starts_with(&canonical_root)` check at path_safety.rs:126.

The existing test `rejects_symlink_escape` (path_safety.rs:283) only
covers the case where the symlink target exists. When the target does
not exist, the same setup is accepted instead of rejected.

The module header (path_safety.rs:8-12) says the helper backs the
argv matcher's `path-under` constraint, per-rule cwd validation, and
future bundled-executable resolution. Any consumer that later
**creates or writes** the validated path goes through the dangling
symlink and materializes or writes `/outside/secret` instead — a
write escape out of the declared root. Consumers that only read get
an ENOENT, so the read case is not exploitable.

## Impact
A gadget (or an attacker able to plant a symlink under a declared
root, e.g. inside its own writable data dir) can pre-create a
dangling symlink and have a "safe" path resolve through it. If the
path is then used as an output path (command writes to it, scaffold-
this-path-later bundled-binary case expl