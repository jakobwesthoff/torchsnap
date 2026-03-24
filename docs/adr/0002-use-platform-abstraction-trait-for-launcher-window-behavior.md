# 2. Use platform abstraction trait for launcher window behavior

Date: 2026-03-24

## Status

Accepted

## Context

The launcher needs platform-specific window management to feel native.
On macOS, this means an NSPanel that receives keyboard input without
activating the owning process — the active app keeps focus while the
user types into the launcher. On Linux and Windows, equivalent native
APIs exist (layer-shell on Wayland, `WS_EX_NOACTIVATE` on Windows)
but differ significantly in their mechanics.

A reference implementation (nutty/squirly) used `compile_error!` to
restrict to macOS only. Torchsnap targets all three platforms from
day one.

## Decision

Define a `LauncherPanel` trait in `src-tauri/src/platform/mod.rs`
with methods `init`, `show`, `hide`, and `is_visible`. Each platform
provides its own implementation:

- **macOS** (`platform/macos.rs`): converts the Tauri window to a
  custom NSPanel subclass via `tauri-nspanel`, configures
  `NonactivatingPanel` style mask, floating window level, and
  all-spaces collection behavior.
- **Fallback** (`platform/fallback.rs`): uses regular Tauri window
  show/hide/focus. Functional but steals focus from the active app.

The correct implementation is re-exported as `PlatformLauncherPanel`
via `cfg(target_os)` dispatch in `mod.rs`. Consuming code in `lib.rs`
calls `PlatformLauncherPanel::show(app)` without any platform
conditionals.

## Consequences

- The project compiles and runs on all platforms from the start.
- Linux and Windows get degraded UX (launcher steals focus) until
  platform-specific implementations are added.
- Adding a new platform implementation is isolated to creating a new
  module and updating the `cfg` dispatch — no changes to `lib.rs`.
- The macOS-only crates (`tauri-nspanel`, `objc2-app-kit`) are
  declared under `[target.'cfg(target_os = "macos")'.dependencies]`
  so they don't affect compilation on other platforms.
