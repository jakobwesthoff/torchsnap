---
kind: feature
status: open
area: [.github/workflows/ci.yml]
---

# GitHub Actions CI: check Linux and Windows compilation

`.github/workflows/ci.yml` runs `just fullcycle` (Rust check,
clippy, frontend build, formatting) on every push to `main` and on
pull requests, gated to `runs-on: macos-latest` since the host code
is mostly macOS-only and releases build on Apple silicon. It skips
private-repository runs entirely (`if: !github.event.repository.private`),
so nothing has run yet.

Still missing: a check that the fallback (non-macOS) code paths
compile on Linux and Windows, so a macOS-only dependency added by
mistake is caught before it breaks those platforms.

## Remaining work

- Add a `cargo check` job (or matrix leg) for `ubuntu-latest` and
  `windows-latest` against the fallback path, no macOS-only deps
  (`tauri-nspanel`, `objc2-app-kit`).
- Linux runner needs Tauri's Linux build dependencies if a full
  `cargo check` on the workspace pulls in the Tauri crate:
  `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`,
  `patchelf`.
- Windows runner needs WebView2 (pre-installed on GitHub's Windows
  runners) if the same applies there.

## Stretch goals

- Full Tauri build (real app bundles) on all three platforms, or via
  `tauri-apps/tauri-action`. Slower than `check` but catches linker
  errors `check` misses.
- Release builds on tag push producing platform bundles attached to
  a GitHub Release. Currently releases are built, signed and
  published from a developer machine on purpose (CLAUDE.md,
  "Releases and signing"), so this would need to fit that model
  rather than replace it.
