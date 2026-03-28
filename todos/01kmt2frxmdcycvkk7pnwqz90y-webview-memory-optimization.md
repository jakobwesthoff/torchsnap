# Webview Memory Optimization for Launcher Window

## Problem

Torchsnap is a launcher app that spends most of its runtime **hidden**. The
main launcher window (a Tauri webview) is created once at startup and kept
alive permanently — hidden/shown via toggle. This means the webview's full
memory footprint is held at all times, even though the user only interacts with
it briefly.

## Current Architecture

- **Launcher window**: Created once at app startup, never destroyed. Uses a
  macOS `NSPanel` (non-activating, floating). Close is intercepted and
  converted to hide.
- **Settings window**: Already uses the create-on-demand / destroy-on-close
  pattern (commit `0129ff6`). Serves as a reference for lazy window lifecycle.
- **Frontend**: React 19 + Vite + Tailwind. Two separate entry points
  (`launcher.html`, `settings.html`). Plugin UI is code-split via
  `React.lazy()`.
- **Bundle sizes**: ~475 KB JS for launcher (222 KB entry + 252 KB shared
  chunk) + lazy plugin chunks.

## Techniques to Evaluate

### 1. Destroy-on-hide / Create-on-show (Settings Window Pattern)

Apply the same pattern already used for the settings window: destroy the
webview when dismissed, recreate it when the shortcut is pressed.

**Pros:**
- Maximum memory savings — no webview process at all when hidden
- Already proven pattern in codebase (settings window)

**Cons:**
- Cold-start latency on each show (webview init + React hydration + plugin load)
- Likely 200-500ms+ delay before UI appears — may feel sluggish for a launcher
- Need the same "react-ready" flash-prevention dance as settings

**Open questions:**
- What is the actual create-to-visible latency on target hardware?
- Can we pre-warm/cache any part of the webview creation?

### 2. WKWebView Memory Pressure / `isInspectable` Tricks (macOS-specific)

On macOS, WebKit-based webviews may respond to system memory pressure by
releasing caches. There may be private API or configuration to hint the
webview to reduce its footprint while hidden.

**Open questions:**
- Does Tauri/WKWebView expose any API to shrink memory while hidden?
- Does `[WKWebView _setPageVisibility]` or similar private API exist?

### 3. Navigation to Blank Page While Hidden

When hiding the launcher, navigate the webview to `about:blank` (or a minimal
empty page). When showing, navigate back to `launcher.html`.

**Pros:**
- WebView process stays alive (faster than full destroy/create)
- JS heap, DOM, and render tree are released

**Cons:**
- Still has some re-navigation latency
- Need to manage React re-mount and state restoration
- WebView process itself still consumes baseline memory

### 4. Visibility-Based DOM Cleanup

Keep the webview alive but have the React app unmount heavy components when
hidden and remount on show. Use the existing `tauri://blur` / `tauri://focus`
events.

**Pros:**
- Fastest show time — webview is warm, only React reconciliation needed
- No webview creation overhead

**Cons:**
- JS heap is still alive (V8/JSC memory not released)
- Savings limited to DOM/render-tree memory, not JS engine overhead
- May not be significant enough to matter

### 5. Hybrid: Delayed Destroy with Idle Timer

Keep the webview alive for a short grace period after hide (e.g. 30-60s). If
the user doesn't re-open within that window, destroy it. On next open,
recreate.

**Pros:**
- Quick successive toggles remain instant
- Long idle periods benefit from full memory release

**Cons:**
- More complex lifecycle management
- Latency surprise: fast most of the time, then suddenly slow

## Measurement Findings

### Baseline Memory Snapshot (launcher hidden, idle)

Measured with macOS `footprint` tool and `ps` while the launcher was hidden
and idle. The app had been running since boot, launcher never shown in this
session.

| Process | Footprint (dirty) | RSS | Peak |
|---------|-------------------|-----|------|
| **torchsnap** (main Rust process) | 41 MB | 144 MB | 45 MB |
| **com.apple.WebKit.WebContent** (JS/DOM) | 146 MB | 236 MB | 275 MB |
| **com.apple.WebKit.GPU** | 16 MB | 71 MB | 66 MB |
| **com.apple.WebKit.Networking** | 6 MB | 16 MB | 8 MB |
| **Total footprint** | **~209 MB** | | |

Note: RSS includes shared/clean pages and is therefore inflated. The
`phys_footprint` from `footprint` is the more accurate measure of actual
physical memory cost.

### WebContent Process Breakdown (146 MB footprint)

The biggest categories from `footprint -v`:

| Category | Dirty | Reclaimable |
|----------|-------|-------------|
| Owned physical footprint (unmapped) (graphics) | 110 MB | 122 MB |
| WebKit malloc (JS heap + internals) | 22 MB | 92 MB |
| MALLOC_SMALL | 3.3 MB | 24 MB |
| JS JIT generated code | 2.5 MB | 528 KB |
| untagged (VM_ALLOCATE) | 2.5 MB | — |

The 110 MB "graphics" footprint is IOSurface backing stores and compositing
layers — WebKit is keeping these warm even though the window is hidden. The
reclaimable column shows WebKit *could* release ~239 MB total if pressured.

### Key Insight

The WebContent process alone uses 146 MB — 3.5x the main Rust process. Any
optimization that doesn't address the WebContent process will have limited
impact. The graphics backing stores (110 MB) are the single largest item and
are entirely unnecessary while hidden.

### Process Identification

macOS WebKit XPC services (WebContent, GPU, Networking) are launched by
`launchd` (ppid=1), not by our process. They cannot be identified via parent
PID.

**Working approach**: `responsibility_get_pid_responsible_for_pid()` from
`libquarantine.dylib` returns the "responsible" app PID for any XPC service.
Verified that all three WebKit processes (70371-70373) correctly map to
torchsnap's PID (70363), while an unrelated GPU process (2285) maps to a
different app.

### Memory Query APIs (no entitlements needed)

**`proc_pid_rusage(pid, RUSAGE_INFO_V6, &buf)`** from `libproc`:
- Works on any process owned by the same user (no special entitlements)
- `ri_phys_footprint` at byte offset 72 = current physical footprint
  (matches `footprint` tool output exactly)
- `ri_lifetime_max_phys_footprint` at byte offset 240 = peak footprint
- Struct defined in `<sys/resource.h>` as `rusage_info_v6` (typedef'd to
  `rusage_info_current`)

**`proc_listallpids(buf, bufsize)`** from `libproc`:
- Enumerates all PIDs on the system
- Combined with `responsibility_get_pid_responsible_for_pid()` this gives
  the full process tree for our app

These APIs are sufficient to build an external memory profiler with zero
changes to torchsnap itself.

### Measurement Tool Plan (deferred)

Build a standalone Rust CLI tool that:
1. Finds the torchsnap main process (by name or PID)
2. Discovers all associated WebKit XPC processes via
   `responsibility_get_pid_responsible_for_pid()`
3. Samples `phys_footprint` via `proc_pid_rusage()` at configurable intervals
   (e.g., 100ms)
4. Drives app state transitions via the control channel (show, query, dismiss)
5. Outputs timestamped CSV or formatted table for analysis

**Blocked on**: Control channel implementation (needed for step 4).

## Decisions

- **Measurement first, optimization later** — we need reliable baselines
  before trying any technique.
- **Control channel over CGEvent simulation** — deterministic, no timing
  hacks, no accessibility permissions, and doubles as a user-facing scripting
  feature.
- **External measurement tool** — avoids polluting app code with diagnostic
  instrumentation that would itself affect the measurements.
