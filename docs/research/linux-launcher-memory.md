# Launcher Memory on Linux

> **Status as of 2026-10-24**
>
> Results of three memory investigations on the Linux testbed
> (2026-10-23 and 2026-10-24): the benchmark on Linux and the cost of
> a maximized launcher, the growth over show/hide cycles, and where
> the memory goes and which reductions work.
>
> What works:
> - WebKitGTK's `DocumentViewer` cache model on the launcher's web
>   context bounds the image cache that grows with every show.
> - Shrinking the launcher to 1x1 while it is still mapped, then
>   hiding it, frees 35 to 40 MiB while hidden.
> - A canvas renderer for the mascot, a frontend change, bounds the
>   image cache on Linux at a wider spread of show times; macOS was not
>   measured.
>
> What does not: ADR 0020's 1x1 shrink after the hide frees nothing on
> Linux. WebKit's CPU rendering paints the launcher only in an opaque
> window. The allocator and WebKit switches tried are worse or below
> detection.

## Setup

- Raspberry Pi 5, 8 GB, Debian 13, kernel 6.18.50+rpt-rpi-2712.
- GNOME Shell and Mutter 48.7 on Wayland, WebKitGTK 2.54.0, GTK
  3.24.49.
- Desktop: one portrait 4K monitor at scale 2, 1080x1920 logical.
  A headless GNOME session with 1280x800@1 plus 2560x1600@2 (or
  1920x1080@1) served for the multi-monitor checks and as a second
  scaling point.
- Builds: the benchmark of the maximized window and the growth runs
  used debug builds; the growth was repeated on a release build; the
  breakdown and the reduction candidates used release builds. The
  arms of the window comparison and of the reduction candidates were
  environment switches of one binary.
- MiB throughout. ADR 0020's macOS figures keep its MB.

## Method

### Tools

- `tools/memsnap` measures on Linux through `tools/memsnap-linux`:
  PSS and RSS from `/proc/<pid>/smaps_rollup` for Torchsnap and every
  process below it (WebKitGTK's web and network processes), the DRM
  clients' memory from `fdinfo` and the dma-buf fds next to it.
  `--also PID` records the compositor apart from Torchsnap's tree.
- `tools/memsnap-report` sums processes of the same name per phase and
  names the metric (`Metric: PSS`).
- `tools/bench-memory` on Linux builds nothing. It starts the binary
  given with `--app` (in a systemd user scope where `systemd-run`
  exists), waits until the launcher reports `status.ready`, runs the
  macOS scenario of 15 phases and writes an events file with every
  control request and the app's `WEBKIT_*` and `GDK_*` variables.
  Linux options: `--app`, `--compositor-pid` and `--env-prefix PREFIX`
  (more variables in the events file). On both platforms `--cycles N`
  adds measured show/dismiss cycles after the scenario and
  `--timing-cycles N` unmeasured cycles timing show to visible content.
- `just testbed-bench-memory DESCRIPTION` runs the benchmark on the
  testbed against its current build and passes `--cycles`,
  `--timing-cycles`, `--env-prefix` and `--headless`. Its data files
  land in `target/linux-testbed/`. `just testbed-build` deploys a
  debug build only; release builds had to be copied to the Pi by hand
  and selected with `TORCHSNAP_TESTBED_APP_BINARY`.

### The decision figure

Torchsnap's memory is judged by **system Shmem from `/proc/meminfo`
plus the anonymous PSS (`Pss_Anon`) of Torchsnap's processes**.

- PSS misses the GPU buffers: GPU buffers that a process does not map
  are not in its PSS. The tree's `Pss_Shmem` is 0 in every snapshot, so
  Shmem and the tree's PSS do not overlap.
- The DRM clients' figures cannot be summed. `drm-total` counts a
  buffer in every client that holds it, and `drm-shared` is that
  overlap. While the launcher is shown, GNOME Shell's shared part grows
  by the UI process's buffers it composites.
- Shmem counts each v3d buffer once, however many processes hold it.
  Per phase, the Shmem difference between two arms matched the UI
  process's GPU total plus the web process's GPU total minus the web
  process's shared part to within 0 to 6 MiB in 14 of the 15 phases
  after the start; in one shown phase Shmem was 31 MiB higher.
- Shmem is system-wide, so anything else that allocates shmem in a run
  shows up in it. On the headless session its spread reached 49 MiB.
- PSS also divides shared libraries with GNOME Shell, whose PSS drifts
  by 17 MiB between runs. The decision figure's noise floor on the
  desktop is a few MiB; that of the tree's and GNOME Shell's PSS is a
  median 13 MiB.

For a breakdown per process, the GPU estimate is the UI process's GPU
total plus the web process's GPU total minus the 21.4 MiB the web
process shares with the UI process.

### Pass rule

- n = 3 per arm and phase. For the reduction candidates the baselines
  ran first and last in every round, arm order rotated.
- An arm passes a figure when its three paired differences to the mean
  of its round's two baseline runs have one sign, its three values lie
  outside all baseline runs, and the mean difference is at least
  12 MiB. Differences under 12 MiB are below detection.
- A null check (each block's first against its last baseline run) gave
  one false positive in 20 tests of the decision figure.
- The comparison of window sizes used a simpler rule: the ranges of the
  three runs apart and the same sign in all three rounds.

### Only a painted launcher counts

With WebKit's hardware-acceleration policy `NEVER`, and with
`GDK_GL=disable` in today's transparent window, the launcher painted
nothing or only fragments. The window was mapped, visible and focused,
the control API answered, and the frontend reported its content as
visible. The benchmark's show-to-content time and the control
`snapshot` report the frontend's state, not pixels, and did not catch
it. A launcher that paints nothing holds no buffers, so the savings of
these arms, and of two arms on the same paths (the policy with the
mapped shrink, and `WEBKIT_DISABLE_DMABUF_RENDERER=1`), are invalid.
`WEBKIT_DISABLE_COMPOSITING_MODE=1` takes the same renderer according
to WebKit's source and was not checked on screen either. A memory
figure counts only for a launcher seen painted on screen, in
screenshots or a recording of the same build and switch.

### Other conditions

- The Pi does not swap on its own. After a reboot, 10.8 minutes idle
  without Torchsnap left zram empty, and none of 68 runs swapped. Swap
  seen before the reboot (GNOME Shell with 131 MiB swapped out) came
  from earlier activity in that boot.
- The recordings run at about 5 frames per second on the 4K screen, so
  they show what is painted. Stutter and tearing are not ruled out for
  any arm.

## Where the memory goes

Release build, desktop, at start (launcher hidden), six runs:

| Process | PSS (anon / file) | GPU total (private) |
|---|---|---|
| UI (`torchsnap`) | 123.8 (61.0 / 62.6) | 78.1 (22.5) |
| WebKitWebProcess | 138.1 (70.6 / 67.4) | 133.4 (111.9) |
| WebKitNetworkProcess | 27.6 | |
| Tree PSS / GPU estimate / sum | 289.4 / 190.0 / 479.4 | |

By owner:

| Part | MiB | Owner |
|---|---|---|
| Web process PSS and its GPU buffers | 271.5 | WebKit; sized by Torchsnap's page and window |
| Network process PSS | 27.6 | WebKit; no setting removes it |
| UI process: libraries (other `.so`, WebKit, JavaScriptCore) | 47.9 | system: GTK, WebKitGTK's UI side, Mesa, flite |
| UI process: `torchsnap` binary and `.cwasm` gadget code | 37.1 | Torchsnap; file pages |
| UI process: heap and other anonymous | 37.6 | the Rust host, wasmtime, GTK and WebKit's UI side; not separable by mapping |
| UI process: GPU buffers not shared with the web process | 56.7 | GTK and WebKit compositing in the UI process |
| Sum | about 479 | |

About 299 MiB are WebKit's processes and buffers, about 105 MiB system
libraries and UI-side buffers, and at most about 75 MiB Torchsnap's own
code, gadgets and heap.

- Per mapping, the UI process holds 31.4 MiB of shared libraries other
  than WebKit, the binary 24.4, `[heap]` 20.9, other anonymous 16.7,
  gadget code 12.7 (five gadgets), libwebkit2gtk 10.7 and
  JavaScriptCore 5.8. The web process holds 44.4 MiB of anonymous
  mappings, libwebkit2gtk 43.3, other libraries 29.3 and JavaScriptCore
  13.4. Mesa's `libLLVM.so.19.1` takes 10.6 MiB in each of the UI and
  web processes.
- The release binary is 79.5 MB, of which `.rodata` is 44.0 MB: the
  embedded frontend, mostly the mascot WebP files.
- The debug build adds about 37 MiB, all file pages in the UI process:
  the binary's mapping is 56.4 against 24.4 MiB and the dev gadgets'
  code 16.9 against 12.7. Anonymous memory and the WebKit processes do
  not differ. Memory work on Linux should measure release.
- The hidden launcher keeps its GPU buffers. Three minutes after a
  dismiss the GPU estimate is still 146 to 157 MiB, flat from 60 s on;
  nothing releases on its own.
- CPU while hidden is about 0.02 % of a core for the UI process in
  every arm measured, with or without GPU rendering.
- Linux app discovery is a stub, so the queries found no apps and no
  icons. A real implementation adds memory not measured here.

## Growth over show/hide cycles

It is a cache, not a leak.

**Cause.** Every show draws a new random mascot
(`src/mascot/useMascotVariant.ts`, the draw on blur). WebKit keeps
each new image as a decoded copy (576 KiB, 384x384x4) in anonymous
memory and as a Skia GPU texture (592 KiB). The UI process serves each
new mascot file from the frontend assets embedded in the binary, whose
pages stay resident.

**Levels.** Over 500 plain cycles above the hidden state before the
first cycle (debug build):

| Field | cycle 50 | 200 | 500 | fitted bound |
|---|---|---|---|---|
| web process anon | +24.0 | +33.9 | +38.0 | 37.7 |
| web process private GPU | +24.6 | +30.4 | +30.5 | 30.4 |
| UI process file pages | +4.3 | +12.0 | +21.1 | 25.4 |
| UI process anon | +3.8 | +3.5 | +3.8 | flat |

The release build behaves the same (web GPU +32.3, web anon +41.2, UI
file +21.7 at 500). The network process does not grow. Queries on every
show (100 cycles) end 7 to 20 MiB higher in web anon and 2 to 5 MiB
higher in web GPU than plain cycles, and flatten the same way.

**Why it is a cache.**

- With five distinct mascot URLs rotating, nothing grows: an image
  already shown costs nothing again. Without the draw, web GPU and UI
  file pages stay flat.
- The GPU part disappears with CPU rendering
  (`WEBKIT_SKIA_ENABLE_CPU_RENDERING=1`) while the anon part stays, so
  the two are separate allocations.
- The JS heap does not grow: WebKit's sampler shows 4.9 to 6.4 MiB in
  use, flat.
- The bound is WebKit's. The GPU level is about 49 textures and 17 to
  20 decoded copies stay, while the all-year pool alone has 584
  variants. Torchsnap holds no reference to earlier images: `Mascot.tsx`
  renders one `<img>`, and the recent list keeps 20 character names as
  strings. Removing each old `<img>` still keeps 18 decoded images.
  Clearing WebKit's memory cache, the `DocumentViewer` cache model or
  revoked blob URLs drop the kept images to 0 or 1 and web GPU to +2.2
  to +2.7 MiB, so the textures live as long as WebKit's memory cache
  keeps their images.
- The UI file pages' fitted bound (25.2 to 25.4 MiB) matches the
  25.3 MiB of the 626 `-384.webp` files. They are clean file pages the
  kernel can drop under memory pressure, although PSS counts them.
  Neither the cache model nor the canvas renderer changes them, and
  they are not worth a change.

A smaller file or another format was not tried: a decoded image takes
width x height x 4 bytes whatever the encoding, and the 384 file is the
size shown at scale 2.

## Levers

Effects on the decision figure, MiB, against the in-round baseline,
release build, unless the row says otherwise. Growth strategies give
the level above the first cycle (debug build); show times are median /
max (or p90) / first over 10 timing cycles. Today's growth for
comparison: web anon +33.9 (+38.0 at 500), web GPU +30.4, show 72 / 79
/ 101 ms. Today's show-to-content in release: 61 ms median.

### Works

| Lever | Effect | Cost | Verdict |
|---|---|---|---|
| `DocumentViewer` cache model on the launcher's web context (X1) | web anon +2.5, web GPU +2.7 at 500 cycles; one other run kept +23.9 anon at 200 (see open questions) | One call through `with_webview` after the window is built, `webkit2gtk = "=2.0.2"` (wry's pin) as a Linux dependency. tauri-runtime-wry shares the context among all webviews without a data directory, so the settings and welcome windows lose their memory cache too. Show 72 to 75 ms median, one 545 ms first show. | Bounds the cache on Linux; paints correctly on three shows with three mascots. WebKitGTK only. |
| Shrink to 1x1 while mapped, wait 150 ms, then hide (C5) | −40 at start, −35 after the first dismiss, −17 after 20 cycles; shown phases unchanged. Frees the window's buffers shared between the UI process, the web process and GNOME Shell (UI shared GPU 40.9 to 8.8, web shared 21.4 to 0); the web process's own 93 MiB of GPU memory stays. | Show-to-content 75 against 61 ms median. The 150 ms wait was fixed and off the main thread. | Paints correctly over five cycles with a new mascot each. Measured with today's 808x860 window only. |
| Canvas renderer: one reused `<canvas>`, `fetch`, `createImageBitmap`, `drawImage`, `ImageBitmap.close()` (X4) | web anon +5.1, web GPU +2.8 at 500 cycles; 3 decoded images kept | A canvas renderer in `Mascot`. Show 77 / 102 / 229 ms in one run, up to 429 ms in the other. | A frontend change; not measured on macOS. Sharp at scale 2, placed as the `<img>`, `aria-hidden` to match today's `alt=""`. The new mascot is drawn 13 to 30 ms after the dismiss, so a show within about 30 ms shows the previous one. |

### Works only with a changed window

| Lever | Effect | Cost | Verdict |
|---|---|---|---|
| `GDK_GL=disable` with an opaque launcher window, against an opaque baseline (C4op) | −118 to −151 in every phase; GPU estimate to 0, tree PSS −19 to −27 | A white 808x860 box around the card. Show-to-content 136 against 53 ms median, web process CPU while shown 27.4 against 5.9 % of a core. Applies to the whole UI process; GTK reads `GDK_GL` once at GDK start, so the app could set it at the top of `main()`. | Not usable without a design change to the window. |

### Partial or not worth it

| Lever | Effect | Verdict |
|---|---|---|
| Clear WebKit's memory cache on every hide (X2) | web GPU +2.2, web anon +25.9 at 200 cycles | Bounds the GPU part only; an asynchronous call on every hide. |
| Blob URL per draw, the previous one revoked (X5) | web GPU +2.3, web anon +26.5 at 200 cycles | Bounds the GPU part only. |
| Web-process memory limit of 300 MB (X6) | web anon +5.0, web GPU +12.8 at 200 cycles | Needs wry to build the context with the setting (construct-only property); show 82 / 186 / 201 ms. |
| A new `<img>` per draw, the old one's `src` and `srcset` removed (X3) | web anon +54.3, web GPU +29.9 at 200 cycles | No gain. |

### No effect, below detection, or worse

| Lever | Effect |
|---|---|
| `WEBKIT_SKIA_GPU_PAINTING_THREADS=1` (C3) | +7 to −13, no pass |
| `MALLOC_ARENA_MAX=2` (C6) | −16 at final idle, one pass (−17 after the first dismiss) in five phases |
| `malloc_trim(0)` after hide (C7) | UI anon −11.8 at final idle, same sign in all rounds, below the floor |
| `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1` (one run) | moves the UI's buffers into the web process, +30 hidden |
| `WEBKIT_SKIA_MSAA_SAMPLE_COUNT=0` (one run) | −6 shown |
| `WEBKIT_DISABLE_DMABUF_ATLAS=1` (one run) | +14 hidden |
| ADR 0020's 1x1 shrink after the hide | nothing, with today's window and with a maximized one (see "The shrink on Linux and on macOS") |

### Broke rendering, or on the same path (figures invalid)

| Lever | What the screen showed |
|---|---|
| WebKit's `hardware-acceleration-policy` `NEVER` on the launcher's webview through `with_webview` (C11) | Nothing; with an opaque window a black box on every show, also 6 s after the show. No WebKit error in the app log. |
| The same policy with the mapped shrink (K) | Not checked on screen; its figures count as invalid with C11's. |
| `GDK_GL=disable` with today's transparent window (C4) | Fragments only: patches of the emoji grid where something changed, no card, no mascot. Settings and welcome windows render. |
| `WEBKIT_DISABLE_DMABUF_RENDERER=1` (C8b) | Not checked on screen; it uses the same CPU path as C4, so its figures count as invalid. |
| `WEBKIT_DISABLE_COMPOSITING_MODE=1` (C2) | Not checked on screen. It measured worse (+25 shown, +30 at final idle and after 20 cycles), but it takes the same non-composited renderer with GL still on (next section), and a downstream report sees fragments with it on 2.54 in a transparent window. Its figures carry the same caveat as C4's and C11's; this is an inference from the source and one report, not a measurement. |

The idle CPU while hidden of C4, C11 and K equals the baseline's.

### Why the CPU paths break

From the WebKit 2.54.0 source (tag `wpewebkit-2.54.0`), not reproduced
on the Pi.

**Fragments in the transparent window.**

- With `GDK_GL=disable` or `WEBKIT_DISABLE_DMABUF_RENDERER=1`, the UI
  process turns off hardware acceleration and accelerated compositing,
  and the web process's drawing area creates `NonCompositedFrameRenderer`
  instead of the layer tree host.
- Each frame, that renderer clears the whole render target to
  transparent when the page background is not opaque, then redraws
  only the damaged rectangles. Everything outside the damage stays
  transparent.
- wry sets the webview background to (0,0,0,0) when the window is
  transparent, and the launcher is built with `.transparent(true)`. So
  every frame is all transparent plus the changed patches: the picture
  seen with C4.
- With an opaque or unset background the clear is skipped, and the
  damage-only draw lands on the previous contents. That fits the
  correct picture in the white box of C4op. The opaque test changed
  both the webview background and the GTK window's RGBA visual, so
  which of the two matters is not separated.
- The composited Skia path clears only the damage region; the
  non-composited path has no such restriction.
- The code has no display-server branch. Four downstream projects
  report the same symptom on WebKitGTK 2.54, one of them also under
  XWayland and one on X11, and work around it with an opaque window.
  The same code is on WebKit `main` as of 2026-10-24. No upstream WebKit
  bug for it was found.

**`NEVER` set after the page loaded.**

- `hardware-acceleration-policy` is a construct-time property. Setting
  it to `NEVER` later flips preferences in the settings' store but
  replaces no running renderer.
- The web process picks its frame renderer once, when the drawing area
  is created, and the render surface copies the GPU flag in its
  constructor. After a late `NEVER` the preferences say software and
  non-composited while the web process keeps the GPU renderer. Which
  mismatch produces the black box is not traced.
- wry builds the webview first and applies settings to the existing
  view, and tauri-runtime-wry's `on_webview_created` is wired for
  Android only. The only route found to set the policy at creation is a
  wry patch that passes a prebuilt settings object to the webkit2gtk
  crate's `WebViewBuilder::settings`.
- Set at creation, the policy leads into the same non-composited
  renderer, so it does not cure the fragments.

**`WEBKIT_DISABLE_COMPOSITING_MODE=1`** switches the page to the same
non-composited renderer, with GL still on.

### Not tried

- Setting the acceleration policy at creation; it needs a wry patch,
  and from the source it ends in the same non-composited renderer.
- Other compositors (labwc, KDE, X11) and another GPU. The renderer
  code above has no display-server branch.
- `webkit_web_context_garbage_collect_javascript_objects()`, which
  `webview-garbage-collection.md` lists for Linux.
- Any of these on macOS.

## A maximized launcher window

Measured with a debug build: today's window (A) against the window
maximized through `gtk_window.maximize()` before every show (B), and
both with and without ADR 0020's shrink. B had no input region and no
card placement; the memory those add was not measured.

**Memory, desktop.** B costs +227 MiB at start, +216 to +234 MiB while
shown and +169 to +184 MiB while hidden, every phase passing. A's
absolute decision figure is 857 to 942 MiB by phase. The difference
sits in the GPU buffers of the UI and web processes (Shmem +141 to +150
hidden, +181 to +213 shown) and in the UI process's anonymous memory
(+21 to +42). The web process's PSS and GNOME Shell's PSS do not change
measurably. Tree GPU peaks reach 468 to 486 MiB for B against 253 MiB
for A. On this monitor B's buffers are about 2.9 times A's (2160x3770
against 1616x1720 px).

**Memory, large landscape monitor.** On the headless 2560x1600@2
monitor the difference is −26 to +46 MiB by phase and often not
separable, since today's window there is already 808x800 logical and
the maximized one 1280x800: about 1.6 times the buffers.

**Time and CPU, desktop.** Show to visible content 104 ms median for B
against 71 ms for A. While shown, GNOME Shell uses 20 % of a core
against 15 %, the UI process 22 % against 8 %; the web process 5 to 7 %
in both.

**Placement.** Maximize follows the pointer's monitor on monitors of
different size and of different scale, on every show. B's first
composited frame is already at the maximized rect; there is no frame at
the small size, the old position or the other monitor. What moves is
GNOME Shell's open animation for normal windows (scale from the bottom
centre, 150 ms): with B the card grows in from the bottom of the work
area. A content flash of one or two frames between recorded frames is
not ruled out.

## The shrink on Linux and on macOS

- **macOS (ADR 0020).** WebContent's memory is dominated by tile stores
  proportional to the webview's pixel size. Resizing the hidden window
  to 1x1 makes WebKit release them: WebContent drops from about 64 to
  about 22 MB after a dismiss.
- **Linux, shrink after the hide.** It frees nothing. Small window with
  and without it, and maximized window with and without it, are not
  separable in any hidden phase; the hidden window's GPU buffers stay
  the same. The 1x1 resize while hidden does not unmaximize the window,
  and the next show is maximized at the work area's size.
- **Linux, shrink while mapped.** Shrinking before the hide frees the
  shared window buffers (−35 to −40 MiB) but not the web process's own
  GPU memory. It was measured with today's small window only, not under
  a maximized one.

## Open questions

- **About 25 MiB of web-process anon that comes and goes between
  runs.** Some 200-cycle runs gained +21 to +30 MiB, others with the
  same setup +0.3 to +5.0 (the cache model gave +23.9 in one run and
  +1.5 in the other). One run peaked at +50.7 at cycle 451 and was back
  at +21.8 at 500, so it is released again. Ruled out: the JS heap, the
  decoded-image mappings, GPU memory. In the runs without the draw it
  sits mostly in one new 30 MiB anonymous region (15 MiB resident);
  which allocator owns it is not established.
- **CPU rendering in a transparent launcher.** The cause above comes
  from the source and is not reproduced on the Pi; separating the
  webview background from the window's visual was not tried. No clean
  way was found to keep the transparent launcher and CPU-render it with
  stock WebKitGTK 2.54. Options found, none verified:
  - make every frame damage the whole viewport while the launcher is
    shown, through a full-viewport element whose style changes every
    frame; it costs continuous full-frame software rendering, and
    whether damage tracking treats such a change as full damage is
    unknown;
  - fix WebKit so the non-composited renderer clears only the damage
    region, as the composited path does; no upstream report of the
    bug was found, and a fix does not reach a distribution's WebKitGTK;
  - an opaque launcher window, known to work (C4op and the downstream
    projects), at the cost of its look and 2.6 times the show-to-content.

  No switch was found that keeps the composited path without GL: the
  composited surface uses GL whatever the hardware flag says.
- **Lowering the GPU floor with GPU rendering on.** From the source:
  the web process frees spare render targets 10 s after the view
  becomes invisible, and its swap chain holds 2 to 4 targets. The
  hidden snapshots were taken at 5, 60 and 180 s, and the first dismiss
  already lowered the GPU estimate from 190 to 160 MiB; whether that
  drop is this timer is not established. Options beside the mapped
  shrink, none measured:
  - terminate the web process after a long hide
    (`webkit_web_view_terminate_web_process`): it frees the web process
    and its GPU buffers (271.5 MiB at start) at the price of a reload
    and a longer next show; whether the launcher recovers its state is
    open;
  - WebKit's memory-pressure settings, applied per web context, which
    purge Skia's cache under pressure. wry creates the context itself
    and exposes no such setting, so this needs a wry patch, and WebKit's
    documentation warns that a low limit with a fast poll can keep the
    process releasing memory constantly.

  Skia's GPU resource cache limit depends on installed RAM and is
  WebKit's default above 4 GB; it is a ceiling, not the usage, and the
  app cannot set it.
- **What the mapped shrink should wait for.** The experiment shrank
  after the page had blanked and waited a fixed 150 ms before hiding.
- **The mapped shrink under a maximized window.** Not measured; the
  shrink after the hide is a no-op there as well.
- **The cache model's shared context.** The settings and welcome
  windows lose their memory cache too; whether they still behave as
  before is not checked.
- **macOS.** What WKWebView keeps per new image, whether the binary's
  embedded mascot assets show up in the app's footprint, and the canvas
  renderer's effect there were not measured. The only macOS data is
  ADR 0020's creep of WebContent across dismiss phases (22, 26, 37,
  39 MB).
- **Other setups.** One machine, GNOME 48.7 on Wayland, one 4K portrait
  monitor at scale 2. Other GPUs, X11, labwc and KDE are not measured.
