---
kind: bug
severity: medium
status: open
area: [src-tauri/src/wasm/argv_matcher.rs]
tags: [unconfirmed, security]
---

# Command-permission globs match across `/` (globset default)

## Problem
`compile_constraint` compiles `glob` argv constraints with globset
defaults (`src-tauri/src/wasm/argv_matcher.rs:167-174`):

```rust
ArgvConstraint::Glob { pattern } => {
    let glob = Glob::new(pattern).map_err(|e| CompileError::Glob { ... })?;
    Ok(CompiledArgvConstraint::Glob(glob.compile_matcher()))
}
```

globset's `GlobBuilder::literal_separator` defaults to `false`,
which means `*` and `?` also match `/`. A manifest rule like
`refs/heads/*` therefore also accepts `refs/heads/a/../../etc`
or any multi-segment string starting with `refs/heads/`, not just
one path segment. Since `[[permissions.command]]` rules are a
security boundary (the argv matcher is documented at the top of
the file as the "security-critical core of the `command` host
capability"), a glob that a gadget author writes with
single-segment intent is silently broader than intended.

globset also enables `{a,b}` alternation and `[...]` character
classes by default, further widening the accepted surface beyond
what an author thinking in shell-glob terms may expect.

## Impact
Command permission rules using `glob` accept more argv values
than the pattern visually suggests. Whether this is exploitable
depends on the binary being invoked; for path-like arguments a
multi-segment match can reach unintended targets. At minimum the
matching semantics are undocumented and surprising in a
permission context.

## Suggested fix
Decide the intended semantics explicitly. If globs should be
segment-scoped (shell-like), build with
`GlobBuilder::new(pattern).literal_separator(true)`. Whatever the
decision, document the exact matching semantics (separator
behavior, alternation, character classes) in the manifest
permission docs, and add matcher tests pinning the behavior
(e.g. does `refs/heads/*` accept `refs/heads/a/b`?).
