---
kind: chore
severity: low
status: open
tags: [linux, windows, ci]
---

# Non-macOS build compiles with five warnings

`cargo check` of `src-tauri` for Linux compiles but warns. With
clippy's `-D warnings`, `just lint` would fail on Linux. Found on
2026-09-25 while fixing the fallback discovery impls:

- `src-tauri/src/gadget_install/mod.rs`: unused import
  `submit_opened_urls` (its only caller in `lib.rs` handles
  `RunEvent::Opened`, which exists on macOS only).
- `src-tauri/src/gadget_install/commands.rs`: `submit_opened_urls` is
  never used, for the same reason. Its tests call it on every
  platform.
- `src-tauri/src/gadgets/system_preferences.rs`: unused import
  `anyhow::Context`, and an unreachable `Ok(PostAction::Dismiss)`
  after the non-macOS `bail!` in `execute`.
- `src-tauri/src/platform/settings_discovery.rs`: field `bundle_path`
  of `SettingsPane` is never read.

Fix without `#[allow]` (project rule): restructure the cfg-gated code
so each platform compiles only what it uses.

## Checking on a Mac

No Linux target is set up locally. A throwaway container type-checks
the Linux build:

```sh
docker run --rm -v "$PWD":/src rust:bookworm bash -c '
  apt-get update -qq &&
  apt-get install -y -qq --no-install-recommends libwebkit2gtk-4.1-dev \
    libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev \
    libssl-dev pkg-config &&
  cd /src/src-tauri && CARGO_TARGET_DIR=/tmp/target cargo check --locked'
```

No CI job or `just` recipe runs this yet.
