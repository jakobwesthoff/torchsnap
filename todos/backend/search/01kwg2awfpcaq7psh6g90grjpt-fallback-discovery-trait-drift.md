---
kind: bug
severity: high
status: open
area: [src-tauri/src/platform/fallback/app_discovery.rs, src-tauri/src/platform/fallback/settings_discovery.rs]
tags: [ci]
---

# Fallback discovery impls implement trait methods that no longer exist — non-macOS build is broken

## Problem

The `AppDiscovery` trait declares exactly two methods
(`src-tauri/src/platform/app_discovery.rs:64,73`):

```rust
fn discover(&self) -> anyhow::Result<Vec<DiscoveredApp>>;
fn icon(&self, app: &DiscoveredApp) -> anyhow::Result<Option<DynamicImage>>;
```

The fallback implementation additionally implements `open` and
`reveal` inside the `impl AppDiscovery for FallbackDiscovery` block
(`src-tauri/src/platform/fallback/app_discovery.rs:28-36`):

```rust
fn open(&self, _entry_id: &str, _app: &tauri::AppHandle) -> anyhow::Result<()> {
    anyhow::bail!("opening applications is not supported on this platform")
}

fn reveal(&self, _entry_id: &str, _app: &tauri::AppHandle) -> anyhow::Result<()> {
    anyhow::bail!("revealing applications is not supported on this platform")
}
```

Likewise `SettingsDiscovery` declares only `discover` and `icon`
(`src-tauri/src/platform/settings_discovery.rs:52,58`), but
`FallbackSettingsDiscovery` implements an extra `open`
(`src-tauri/src/platform/fallback/settings_discovery.rs:28-30`).

Implementing a method that is not a member of the trait is a hard
compile error (E0407). The fallback modules are gated with
`#[cfg(not(target_os = "macos"))]` (`src-tauri/src/platform/mod.rs:43-44`),
and the macOS implementations correctly implement only the two
current trait methods (`macos/app_discovery.rs:106,135`,
`macos/settings_discovery.rs:39,73`). Development happens on macOS,
so the drift is invisible: the broken code is never compiled.

Evidently the traits once had `open`/`reveal` methods that were
removed (launch/reveal now goes through other paths; no caller of
`discovery.open(...)`/`.reveal(...)` exists under
`src-tauri/src/gadgets/`), and the fallback impls were not updated.

## Impact

Any attempt to build for Linux or Windows fails to compile. More
generally it demonstrates that the `cfg(not(macos))` half of the
platform layer has no compile coverage at all, so any trait change
can silently break it.

## Suggested fix

1. Delete the orphaned `open`/`reveal` methods from both fallback
   impls.
2. Consider adding a CI job (or a `just` recipe) that runs
   `cargo check --target x86_64-unknown-linux-gnu` (or
   `x86_64-pc-windows-msvc`) for `src-tauri` so the fallback path
   gets at least type-checked. If cross-target checking is too
   heavy, a lighter alternative is `cargo check` with
   `--cfg torchsnap_force_fallback` style feature gating, but a
   real target check is the only thing that catches this class of
   bug reliably.
