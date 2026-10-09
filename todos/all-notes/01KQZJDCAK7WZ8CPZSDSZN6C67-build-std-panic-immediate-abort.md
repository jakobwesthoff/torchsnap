---
kind: improvement
status: open
tags: [performance]
---

# Nightly `build-std` + `panic_immediate_abort` for gadget builds

## Problem

Even with `panic = "abort"`, Rust still emits the panic *formatting*
machinery: every `unwrap`, `expect`, slice index, integer overflow
check, and `Debug`/`Display` for the panic payload pulls in
`core::fmt`. On wasm this means a sizeable chunk of code that runs
exactly once (during a panic, immediately before abort) and is
otherwise dead weight.

`core::fmt` typically accounts for **30–50 %** of small Rust wasm
binaries. For our gadget workspace this is the largest single
size lever still available after `wasm-opt`, profile tuning, and
dependency dieting — but it requires nightly Rust and a workspace
discipline around toolchain pinning.

## Goal

Build the gadget workspace with:

```
cargo +nightly build --release \
  -Z build-std=std,panic_abort \
  -Z build-std-features=panic_immediate_abort
```

This rebuilds `std` (and `panic_abort`) for `wasm32-wasip2` with the
`panic_immediate_abort` feature, which replaces every panic-formatting
path with a direct `core::intrinsics::abort()` (a bare wasm
`unreachable`). The strings, the formatter, the panic handler — all
gone.

## Implementation

### Toolchain pinning

Add (or extend) `gadgets/rust-toolchain.toml`:

```toml
[toolchain]
channel = "nightly-YYYY-MM-DD"  # pinned, not floating
components = ["rust-src", "rustfmt", "clippy"]
targets = ["wasm32-wasip2"]
```

`rust-src` is required for `build-std`. Pin a date so CI is
reproducible; bump intentionally rather than drifting on
`channel = "nightly"`.

The host crate (`src-tauri/`) keeps its own stable toolchain. Cargo
respects the nearest `rust-toolchain.toml` walking up from the
invocation directory, so `cd gadgets && cargo build` picks up
nightly while `cd src-tauri && cargo build` keeps stable. Verify
this isolation works in CI.

### Cargo configuration

Extend `gadgets/.cargo/config.toml`:

```toml
[build]
target = "wasm32-wasip2"

[unstable]
build-std = ["std", "panic_abort"]
build-std-features = ["panic_immediate_abort"]
```

This makes `cargo build` inside `gadgets/` apply the flags
automatically — `just build-gadget` does not need new arguments.

### Justfile / CI plumbing

- `just doctor` should detect the pinned nightly is installed and
  the `rust-src` component is present.
- `just check-gadgets` uses the nightly toolchain (transparent via
  `rust-toolchain.toml`).
- CI: ensure the GitHub Actions setup-rust step installs the pinned
  channel + `rust-src`.

## Verification

- Build all gadgets, record before/after `.wasm` sizes.
- Trigger a deliberate panic in a test fixture (e.g. an integer
  overflow in debug, a slice-out-of-bounds) and confirm wasmtime
  surfaces it as a generic trap (not a formatted panic message).
  This is the *expected* behaviour — `panic_immediate_abort`
  removes the message.
- Check that error paths in production gadget code don't rely on
  panic-message inspection. Any host log scraping for panic strings
  needs to handle the trap-only case.

## Acceptance criteria

- `gadgets/rust-toolchain.toml` pins a specific nightly date.
- `gadgets/.cargo/config.toml` enables `build-std` +
  `panic_immediate_abort`.
- All gadgets build with `just build-gadgets` without explicit
  flags.
- Host crate (`src-tauri/`) is unaffected and continues to build on
  stable.
- CI installs the right toolchain and runs cleanly.
- Size delta documented in the commit (expect 30–50 % shrink on
  smaller gadgets, less on large dep-dominated ones since their
  size is in the deps, not panic infrastructure).
- ADR added (or 0044 extended) recording the nightly dependency and
  the upgrade cadence.

## Trade-offs

- **Nightly dependency.** Every contributor and CI runner now needs
  nightly Rust available for gadget builds. The host crate stays on
  stable, so day-to-day Tauri development is unaffected.
- **Lossy panic info.** Aborts in gadgets become opaque — no message,
  no file/line — only a wasmtime trap. This is acceptable in
  production but can hurt debugging. Mitigation: provide a
  `dev-trap-info` Cargo feature in the SDK that *disables*
  `panic_immediate_abort` for local debugging by overriding the
  `[unstable]` config for that build (manual `cargo +nightly build`
  with explicit flags).
- **Toolchain churn.** Nightly nightly-bumps occasionally break
  `build-std`; pin a date and bump deliberately every few months
  rather than tracking `nightly`.
- Do this *last* among the size todos: it's the most invasive (new
  toolchain dependency), so verify the cheaper options first
  (`wasm-opt`, profile, deps) and reach for this only if the
  remaining gap is worth the operational cost. The decision can be
  re-evaluated when `panic_immediate_abort` stabilises (no current
  ETA upstream).
