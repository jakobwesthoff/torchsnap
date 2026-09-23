# Two near-identical lexical path normalizers in wasm module

**Kind:** refactor
**Severity:** low
**Area:** src-tauri/src/wasm/source.rs

## Problem
`source.rs` defines `normalize_path`
(`src-tauri/src/wasm/source.rs:250-264`) and `path_safety.rs`
defines `lexically_normalize`
(`src-tauri/src/wasm/path_safety.rs:143-157`). Both walk
`Path::components()` and resolve `CurDir`/`ParentDir` lexically.
They differ in exactly one branch: when `pop()` fails on a
leading `..`, `source.rs` drops the component while
`path_safety.rs` preserves it.

The module comment in `path_safety.rs:21-24` explicitly
acknowledges the coexistence of the two *validators*
(`validate_gadget_path` vs `canonical_under_root`) as solving
different problems, which is fine. The duplicated *normalizer*
helper is a separate matter: same algorithm written twice with a
silent behavioral divergence, and each has its own test suite
(`source.rs:898-925`, `path_safety.rs` tests).

## Impact
Maintenance drift risk: a fix to one normalizer (e.g. around
`..` retention or prefix handling) will not reach the other.
Today both call sites happen to guard their inputs so the
divergent branch is unreachable, but that invariant lives only
in comments.

## Suggested fix
Extract one normalizer (the conservative `path_safety.rs`
variant, which preserves unresolvable `..`) into a shared
location and use it from both. Keep both validators as they are.
