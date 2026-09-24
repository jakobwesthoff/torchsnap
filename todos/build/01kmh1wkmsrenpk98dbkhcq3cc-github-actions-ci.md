---
kind: feature
status: open
---

# GitHub Actions CI pipeline

Set up automated builds to catch cross-platform compilation
breakage early. We develop on macOS but must ensure Linux and
Windows continue to compile.

## Minimum viable pipeline

Trigger on push to `main` and on pull requests:

1. **Rust check** (all platforms):
   - `cargo check` on macOS, Linux, Windows
   - macOS is the full check (includes tauri-nspanel, objc2-app-kit)
   - Linux/Windows check the fallback path (no macOS-only deps)
2. **Frontend build**:
   - `bun install && bun run build` (typecheck + vite build)
   - Only needs to run on one platform (output is the same)
3. **Clippy**:
   - `cargo clippy` on macOS (superset of all code paths)
4. **Formatting**:
   - `cargo fmt --check`
   - Could add prettier check for frontend

## Stretch goals

- **Full Tauri build** on all three platforms (produces actual
  app bundles). Slower but catches linker errors that `check`
  misses.
- **Release builds** on tag push — produce macOS `.dmg`, Linux
  `.AppImage`/`.deb`, Windows `.msi`/`.exe` and attach to GitHub
  Release.
- **Tauri's official GitHub Action** (`tauri-apps/tauri-action`)
  handles cross-platform builds and artifact upload.

## Caching

- Cache `~/.cargo/registry` and `target/` between runs
- Cache `node_modules/` (bun lockfile hash as key)
- macOS runners are expensive — minimize macOS-only steps

## Matrix

```yaml
strategy:
  matrix:
    os: [macos-latest, ubuntu-latest, windows-latest]
```

## Dependencies on runners

- macOS: Xcode CLI tools (pre-installed on GitHub runners)
- Linux: `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`,
  `librsvg2-dev`, `patchelf` (Tauri Linux deps)
- Windows: WebView2 (pre-installed on modern Windows runners)
- All: Rust toolchain (via `dtolnay/rust-toolchain`), bun
  (via `oven-sh/setup-bun`)
