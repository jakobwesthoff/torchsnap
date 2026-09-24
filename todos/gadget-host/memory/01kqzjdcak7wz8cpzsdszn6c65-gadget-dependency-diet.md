---
kind: improvement
status: open
tags: [performance]
---

# Audit and slim heavy gadget dependencies

## Problem

The three largest gadgets in the workspace are dominated by
dependency code, not their own logic. Measured `.wasm` sizes from
`gadgets/target/wasm32-wasip2/release/`:

| gadget         | size     | notable deps                               |
|----------------|----------|--------------------------------------------|
| calculator     | 1.63 MB  | regex, evalexpr, blake3, serde_json, ulid  |
| emoji-picker   | 1.37 MB  | (TBD — audit)                              |
| open-url       | 1.20 MB  | (TBD — audit)                              |
| hello-world    |  232 KB  | floor: wit-bindgen + serde_json + SDK      |

`hello-world` represents the practical floor imposed by
`wit-bindgen` + the SDK + `serde_json`. Everything above that floor
is dependency choice, and the calculator alone is **~7×** the floor.

Cargo profile tweaks and `wasm-opt` get us a multiplier on whatever
code is present, but they cannot remove a regex Unicode table that
isn't used. The biggest absolute wins live here.

## Goal

For each gadget over ~400 KB, audit the dependency graph and remove
or replace heavy crates whose features the gadget does not need.
Target: bring calculator, emoji-picker, and open-url under ~600 KB
each (post-`wasm-opt`).

## Concrete candidates

### `regex` → `regex-lite`

`regex` pulls Unicode case-folding tables, SIMD code paths (mostly
inert on wasm but still emitted), and the full NFA/DFA hybrid.
`regex-lite` (same authors, same crate family) drops Unicode and
SIMD for a tiny matcher with the same `regex::Regex` API surface.
Calculator's regex usage almost certainly does not need
Unicode-aware case folding. Saving: typically 200–400 KB.

If `regex-lite` is too restrictive, fall back to `regex` with
features pruned:

```toml
regex = { version = "1.11", default-features = false, features = ["std"] }
```

This drops `unicode-case`, `unicode-perl`, `unicode-bool`, etc.

### `serde_json` → narrower options

Where the gadget controls both ends of the JSON wire (i.e. host ↔
gadget over WIT, not user-facing JSON parsing), consider:

- `serde-json-core` — `no_std`, no allocator pressure, fixed-size
  buffers. Best when payloads are small.
- `miniserde` — minimal derive-based serializer.

If `serde_json` stays, set `default-features = false, features = ["alloc"]`
to drop the `std` feature where unnecessary.

### `blake3` → cheaper hashes

`blake3` brings SIMD code paths and a relatively large state machine.
If the gadget uses it for a non-cryptographic cache key (very
likely in calculator), replace with `xxhash-rust` (xxh3) or
`siphasher`. Saving: 100–200 KB. Keep `blake3` only where
cryptographic strength is actually required — and document why
inline.

### `ulid`

`ulid` typically brings `chrono` and `uuid` interop unless features
are disabled. Use:

```toml
ulid = { version = "1.2", default-features = false }
```

Or replace with a hand-rolled 16-byte ULID using `getrandom` and
Crockford base32 — ~50 lines. Worth it if the only ULID consumer is
internal.

### `serde` baseline

For every gadget, audit:

```toml
serde = { version = "1", default-features = false, features = ["derive", "alloc"] }
```

The `std` feature is on by default and rarely needed.

### `evalexpr`

Calculator-specific. Audit features and consider whether a smaller
expression evaluator (or a hand-written shunting yard) suffices. This
is invasive; treat as a last resort.

## Process per gadget

1. `cargo bloat --release --target wasm32-wasip2 --crates`
   (run from `gadgets/<name>/`) to get the actual size attribution
   per dependency. Trust this output, not the dependency list — some
   crates pull in heavy transitives that don't show up at the top
   level.
2. For the top three offenders, check feature flags. Try
   `default-features = false` first; that alone often shaves
   100–300 KB.
3. Where features aren't enough, evaluate replacement crates from
   the candidate list above.
4. Re-build, record new size, run gadget tests / smoke check.
5. Commit per gadget — atomic commits keep regressions bisectable.

## Acceptance criteria

- `cargo bloat` output captured in the commit for each touched
  gadget (before/after).
- Each gadget over ~400 KB has at least one dependency change
  evaluated and either applied or explicitly rejected with a note.
- No behavioural regressions in the gadget's user-facing flows.
- Replacement crates are documented inline where the choice is
  non-obvious (e.g. `// xxh3 instead of blake3 — non-cryptographic
  cache key`).

## Trade-offs

- Replacing `regex` with `regex-lite` means losing Unicode case-fold
  matching. Acceptable for command syntax matchers; not acceptable
  for user-text search. Decide per gadget.
- Cryptographic hashes are *not* swappable when used for content
  addressing across systems — only for in-process caches.
- Hand-rolled replacements are smaller but carry maintenance cost.
  Prefer feature-flag pruning and `regex-lite`-style drop-ins first.
- This todo is best executed *after* the `wasm-opt` and Cargo
  profile todos, so the baseline numbers are already as small as
  the build pipeline can make them — that way attribution from
  dependency changes is clean.
