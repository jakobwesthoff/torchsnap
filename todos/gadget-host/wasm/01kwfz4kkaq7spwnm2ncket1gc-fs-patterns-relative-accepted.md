---
kind: bug
severity: low
status: open
area: [src-tauri/src/wasm/manifest/permissions/filesystem.rs]
tags: [unconfirmed]
---

# Filesystem read patterns accept relative paths at parse time

## Problem
`validate_fs_pattern`
(`src-tauri/src/wasm/manifest/permissions/filesystem.rs:77-100`)
checks non-emptiness, `..` segments, and `${...}` variable
references. It never checks that the pattern is absolute (or
variable-anchored), even though the traversal error message
explicitly instructs "declare absolute paths only" (`:88-91`).

`read = ["myapp/config.toml"]` therefore parses successfully.
What it means is decided later by
`host::fs::compile_fs_patterns` (referenced in the comment at
`:73-76`): either a compile error at bridge construction (late
failure for an authoring mistake) or a pattern relative to some
implicit cwd (undefined grant).

## Impact
The stated contract (absolute paths only) is not enforced where
the author gets the friendliest feedback. Failure surfaces at
gadget enable time or as a never-matching grant.

## Suggested fix
In `validate_fs_pattern`, require the pattern to start with `/`
or `${` (a substitution variable, which always resolves to an
absolute root). Add a parse test for the relative-path
rejection. While in there: the doc comment (`:20-22`) promises
`*` is single-segment and `**` multi-segment; make sure
`compile_fs_patterns` compiles with `literal_separator(true)`,
and pin that with a test (see also the related glob-semantics
todo for the argv matcher,
`01kwfz4kkaq7spwnm2ncket1fr-glob-matches-path-separator.md`).
