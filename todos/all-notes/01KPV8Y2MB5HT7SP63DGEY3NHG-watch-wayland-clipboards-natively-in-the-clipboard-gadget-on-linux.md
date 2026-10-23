---
title: "Watch Wayland clipboards natively in the clipboard gadget on Linux"
kind: improvement
component: gadgets
---

# Watch Wayland clipboards natively in the clipboard gadget on Linux

The clipboard gadget watches the clipboard through `clipboard-rs` 0.3.3,
which on Linux speaks X11 only. On a Wayland session it works through the
XWayland bridge, which is lossy at the edges. The fix is to read the
Wayland data-control protocol directly when it is available, and fall back
to X11 otherwise.

## Context

`src-tauri/Cargo.toml` pins `clipboard-rs = "0.3.3"`. On Linux this crate
uses **X11 only**: its Linux backend speaks `xcb` against the X server and
knows nothing about Wayland data-device protocols. On a Wayland session the
crate works today because GNOME Mutter's XWayland bridge mirrors most
clipboard operations between the Wayland and X11 clipboards, so
clipboard-rs sees the X11 side and the bridge gives it a reasonable
approximation of what Wayland-native apps copied.

This works well enough most of the time but is lossy at the edges:

- Primary selection is mostly X11-only. Wayland has a separate
  primary-selection protocol (`zwp_primary_selection_device_v1`) that the
  XWayland bridge treats inconsistently.
- MIME-type enumeration is truncated. Not every Wayland offer makes it
  through the bridge with all of its advertised types.
- Large-payload transfers can race the bridge, producing truncated reads or
  missed updates.
- Wayland-only sessions (no XWayland) would see zero clipboard activity.
  GNOME on ARM servers and some minimal Fedora spins are the nearest current
  examples.
- Setup failures (broken Xauth, absent XWayland, headless session) return an
  error and disable the gadget for the session rather than crashing the app.

## Proposal

On Linux, go directly to the Wayland data-control protocol when available,
and fall back to X11. There are two realistic paths.

### Option A: `wl-clipboard-rs` crate

Pure-Rust implementation of `wlr-data-control` (the read-only clipboard
monitoring protocol originally from wlroots, now implemented by GNOME
Mutter 45+, KDE KWin, sway and others). It speaks native Wayland via
`wayland-client`, makes zero shell-outs, and streams clipboard changes
asynchronously.

- Add `wl-clipboard-rs` as a Linux-only dependency.
- Behind `#[cfg(target_os = "linux")]`, detect `$WAYLAND_DISPLAY` at
  startup and prefer the Wayland backend. Fall back to `clipboard-rs` when
  Wayland is unavailable (pure X11 session).
- Mutter 45+ ships `wlr-data-control`. Older GNOME (Fedora before about 39)
  does not, so detect that and degrade gracefully.

### Option B: shell out to `wl-clipboard`

`wl-paste --watch <cmd>` invokes `<cmd>` every time the clipboard changes.
It is straightforward to integrate and works wherever `wl-clipboard` is
installed, but it introduces a subprocess and all of its fragility
(lifecycle, error recovery, MIME-type plumbing via CLI flags). Reasonable as
a hack, not the answer for a production clipboard-history feature.

Recommendation: **Option A**. It is protocol-level integration with the same
quality as native GTK or Qt apps, and it has no process-management hazards.

## Scope of changes

- A new Linux-specific backend inside `src-tauri/src/gadgets/clipboard/`
  that implements whatever trait or channel shape the existing
  `ClipboardWatcher` exposes. The current code is written against
  `clipboard-rs`'s `ClipboardHandler` interface, so the abstraction layer may
  need a small refactor for the Wayland backend to slot in without disturbing
  macOS or Windows.
- Feature detection and backend selection at gadget startup.
- Testing on GNOME Wayland (Mutter 45+), KDE Wayland, sway, labwc and
  classic X11 (e.g. `GDK_BACKEND=x11 just start` or an Xfce session). The
  claim above that Mutter ships `wlr-data-control` is not verified in this
  project; check which data-control protocol each compositor advertises
  (for example with `wayland-info`) before relying on it.

## Pointers

- `src-tauri/src/gadgets/clipboard/mod.rs`: watcher thread. Setup points are
  at lines 159 to 167, the active-query read at line 502.
- `src-tauri/Cargo.toml`: `clipboard-rs = "0.3.3"` dependency.
- `wl-clipboard-rs` on crates.io: <https://crates.io/crates/wl-clipboard-rs>
- `wlr-data-control` protocol: <https://wayland.app/protocols/wlr-data-control-unstable-v1>

## Non-goals

- Replacing `clipboard-rs` on macOS or Windows. Both platforms have
  first-class support in that crate and no reason to switch.
