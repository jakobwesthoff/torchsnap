# 20. Size launcher webview to content and shrink on hide

Date: 2026-03-28

## Status

Accepted

## Context

Torchsnap's launcher window is a Tauri webview backed by macOS WebKit
(WKWebView). WebKit spawns three XPC child processes — WebContent (JS/DOM
rendering), GPU (compositing), and Networking — each with their own memory
footprint. The WebContent process is by far the most expensive, and its
memory cost is dominated by IOSurface-backed tile stores whose size is
proportional to the webview's pixel dimensions.

The original implementation sized the launcher window to fill the entire
monitor (e.g. 2560×1440 on a Retina display = ~14.7M physical pixels) with
a transparent background, using CSS to center a 680px-wide card within it.
This was convenient — it allowed click-to-dismiss on the transparent
backdrop and trivial centering via CSS — but it forced WebKit to allocate
backing stores for the full monitor area even though only ~670K logical
pixels contained actual content.

Benchmark data showed the WebContent process alone consuming 145 MB on
average during a typical usage session, peaking at 232 MB. Its memory grew
monotonically across show/dismiss cycles and was never reclaimed while the
window remained at full size.

## Decision

Two complementary optimizations:

### 1. Size the window to its maximum content bounds

Instead of filling the monitor, the launcher window is set to a fixed size
that tightly wraps the launcher card at its maximum content height (8 result
rows + search bar + footer), plus padding for:

- **CSS box-shadow bleed** (`LAUNCHER_SHADOW_PADDING = 64 px`): the largest
  shadow is `0 16px 48px`, requiring ~64px clearance on each side.
- **Decorative mascot headroom** (`LAUNCHER_MASCOT_HEADROOM = 160 px`):
  the center-mode mascot image extends 156px above the card.

Resulting window dimensions: **808 × 830 logical pixels** — roughly 82%
smaller than a typical 1440p monitor.

The window is centered horizontally on the monitor under the cursor and
positioned so the card appears at ~25% of screen height, matching the
previous visual placement. The CSS uses a fixed `pt-[224px]` (shadow +
mascot headroom) instead of the previous `pt-[25vh]`.

Layout constants are defined in `src-tauri/src/lib.rs` and derived from
the card width (`LAUNCHER_CARD_WIDTH = 680`), maximum card height
(`LAUNCHER_CARD_MAX_HEIGHT = 542`), and the two padding values above.
The CSS top offset must stay in sync with the Rust-side
`LAUNCHER_CARD_TOP_OFFSET`.

### 2. Shrink to 1×1 px on hide

When the launcher is dismissed, the window is resized to 1×1 logical
pixels *after* being hidden. This prompts WebKit to deallocate the tile
backing stores that were needed at the larger size. When the launcher is
shown again, `position_launcher_on_cursor_monitor()` restores the full
window dimensions before the panel becomes visible.

All hide paths — hotkey toggle, frontend blur/click dismiss, and Control
API hide/dismiss — go through `hide_launcher()` which performs both the
panel hide and the shrink. The frontend calls the `launcher_hide` Tauri
command rather than `appWindow.hide()` directly to ensure the shrink is
never bypassed.

## Benchmark Results

All runs used the same scenario: idle → show/dismiss cycle → app search
(Safari, Terminal) → emoji picker (:rocket, :fire) → 5 rapid show/dismiss
cycles → settings query → final idle. Measured with `tools/bench-memory`.

### Continuous sampling (min/max/avg over ~55s)

| Configuration          | WebContent Avg | WebContent Peak | Total Avg | Total Peak |
|------------------------|----------------|-----------------|-----------|------------|
| Baseline (full screen) | 145 MB         | 232 MB          | 215 MB    | 335 MB     |
| Sized to content       | 74 MB          | 110 MB          | 139 MB    | 204 MB     |
| + Shrink on hide       | 55 MB          | 95 MB           | 120 MB    | 187 MB     |

### Phase snapshots (idle states after dismiss)

| Configuration          | After 1st dismiss | After search dismiss | Final idle | Δ first→last |
|------------------------|--------------------|----------------------|------------|--------------|
| Baseline (full screen) | 133 MB             | 137 MB               | 148 MB     | +135 MB      |
| Sized to content       | 64 MB              | 68 MB                | 78 MB      | +64 MB       |
| + Shrink on hide       | 22 MB              | 26 MB                | 39 MB      | +35 MB       |

Key observations:

- **Sizing alone halved** WebContent's average footprint (145 → 74 MB) and
  peak (232 → 110 MB). This confirms the backing-store allocation is
  proportional to viewport area.
- **Shrink-on-hide** changed the memory profile from a **staircase**
  (monotonically growing, never reclaimed) to a **sawtooth** (reclaimed on
  each dismiss, growing only from a low baseline). WebContent drops from
  ~64 MB after dismiss to ~22 MB with shrink — WebKit actively releases
  tiles when the viewport becomes trivially small.
- **GPU process** also benefits: minimum dropped from 11 MB to 5.6 MB as
  compositing buffers are released.
- The slight upward creep across dismiss phases (22 → 26 → 37 → 39 MB)
  suggests some retained JS heap / layout state accumulates, but the bulk
  of graphics memory is freed each cycle.
- **Total final idle footprint: 218 MB → 146 MB → 107 MB** — roughly 50%
  reduction from baseline with zero functional impact.

## Consequences

- **Click-to-dismiss still works** on the transparent padding area within
  the now-smaller window. Clicks outside the window trigger dismiss via the
  existing `tauri://blur` handler.
- The layout constants must be kept in sync between Rust (`lib.rs`) and
  CSS (`Launcher.tsx`). A mismatch would cause the card to appear at the
  wrong vertical position or be clipped.
- The 1×1 shrink is invisible to the user since it happens after the panel
  is already hidden. There is no visual flicker.
- Multi-monitor support is preserved — the window is sized and positioned
  per-show based on the monitor under the cursor.
- The fixed window height accommodates the maximum card size (8 result
  rows). When fewer results are shown, the extra space is transparent and
  invisible.
- Further techniques from the investigation (destroy-on-hide, navigate to
  blank, DOM cleanup) were not needed given the results achieved. They
  remain available if the residual ~39 MB idle footprint becomes a concern.
