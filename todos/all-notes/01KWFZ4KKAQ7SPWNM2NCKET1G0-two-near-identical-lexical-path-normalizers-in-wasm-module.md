---
title: "Two near-identical lexical path normalizers in wasm module"
kind: refactor
component: gadget-host
impact: low
---
# Two near-identical lexical path normalizers in wasm module

`src-tauri/src/wasm/source.rs` and `src-tauri/src/wasm/path_safety.rs` each define a lexical normalizer that walks `Path::components()` and resolves `.` and `..`. They differ in exactly one branch: when `pop()` fails on a leading `..`, `source.rs` drops the component while `path_safety.rs` keeps it. That divergence is silent, and a fix to one will not reach the other.

## Problem
`normalize_path` (`src-tauri/src/wasm/source.rs`) and `lexically_normalize` (`src-tauri/src/wasm/path_safety.rs`) implement the same algorithm. The module comment in `path_safety.rs` (near the top) accepts the coexistence of the two validators (`validate_gadget_path` and `canonical_under_root`) as solving different problems, which is fine. The duplicated normalizer helper is a separate matter. Each copy has its own test suite (`source.rs`, and the tests in `path_safety.rs`).

## Impact
Maintenance drift risk: a fix to one normalizer (for example around `..` retention or prefix handling) will not reach the other. Today both call sites guard their inputs so the divergent branch is unreachable, but that invariant lives only in comments.

## Proposal
Extract one normalizer into a shared location and use it from both. Take the conservative `path_safety.rs` variant, which preserves unresolvable `..`. Keep both validators as they are.
