# Wayland-aware clipboard watcher on Linux

## Context

`src-tauri/Cargo.toml` pins `clipboard-rs = "0.3.3"`. On Linux this
crate uses **X11 only** — its Linux backend speaks `xcb` against the
X server and knows nothing about Wayland data-device protocols. On a
Wayland session the crate works today because GNOME Mutter's
XWayland bridge mirrors most clipboard operations between the
Wayland and X11 clipboards, so clipboard-rs sees the X11 side and
the bridge gives it a reasonable approximation of what Wayland-native
apps copied.

This works well enough most of the time but is lossy at the edges:

- **Primary selection** is mostly X11-only; Wayland has a separate
  primary-selection protocol (`zwp_primary_selection_device_v1`)
  that the XWayland bridge treats inconsistently.
- **MIME-type enumeration** is truncated — not every Wayland offer
  makes it through the bridge with all of its advertised types.
- **Large-payload transfers** can race the bridge, producing
  truncated reads or missed updates.
- **Wayland-only sessions** (no XWayland) would see zero clipboard
  activity. GNOME on ARM servers and some minimal Fedora spins are
  the nearest current examples.
- **Setup failures bubble up as startup panics** when Xauth is
  broken, XWayland is absent, or the user is running headless. See
  `todos/fedora/01kpv8y2ma927eh47xf09mgqp5-clipboard-init-should-not-panic.md`
  for the graceful-degradation side of this.

## What the proper fix looks like

On Linux, go directly to the Wayland data-control protocol when
available and fall back to X11. Two realistic paths:

### Option A — `wl-clipboard-rs` crate

Pure-Rust implementation of `wlr-data-control` (the read-only
clipboard monitoring protocol originally from wlroots, now
implemented by GNOME Mutter 45+, KDE KWin, sway, etc.). Speaks
native Wayland via `wayland-client`, zero shell-outs, streams
clipboard changes asynchronously.

- Add `wl-clipboard-rs` as a Linux-only dep.
- Behind `#[cfg(target_os = "linux")]`, detect `$WAYLAND_DISPLAY` at
  startup and prefer the Wayland backend; fall back to
  `clipboard-rs` when Wayland is unavailable (pure X11 session).
- Mutter 45+ ships `wlr-data-control`. Older GNOME (Fedora < ~39)
  does not — detect and gracefully degrade.

### Option B — shell out to `wl-clipboard`

`wl-paste --watch <cmd>` invokes `<cmd>` every time the clipboard
changes. Straightforward to integrate, works wherever `wl-clipboard`
is installed, but introduces a subprocess and all of its
associated fragility (lifecycle, error recovery, MIME-type
plumbing via CLI flags). Reasonable as a hack; not the answer for
a production clipboard-history feature.

Recommendation: **Option A** — protocol-level integration, same
quality as native GTK/Qt apps, no process-management hazards.

## Scope of changes

- New Linux-specific backend inside
  `src-tauri/src/gadgets/clipboard/` that implements whatever
  trait / channel-shape the existing `ClipboardWatcher` exposes.
  The current code is written against `clipboard-rs`'s
  `ClipboardHandler` interface; the abstraction layer may need a
  small refactor so the Wayland backend can slot in without
  disturbing macOS or Windows.
- Feature detection + backend selection at gadget startup.
- Testing on: GNOME Wayland (Mutter 45+), KDE Wayland, sway, and
  classic X11 (e.g. `GDK_BACKEND=x11 just start` or an Xfce
  session).

## Pointers

- `src-tauri/src/gadgets/clipboard/mod.rs` — watcher thread, setup
  points at lines 159–167, active-query read at line 502
- `src-tauri/Cargo.toml` — `clipboard-rs = "0.3.3"` dep
- `wl-clipboard-rs` on crates.io:
  <https://crates.io/crates/wl-clipboard-rs>
- `wlr-data-control` protocol:
  <https://wayland.app/protocols/wlr-data-control-unstable-v1>

## Non-goals

- Replacing `clipboard-rs` on macOS or Windows. Both platforms have
  first-class support in that crate and no reason to switch.
