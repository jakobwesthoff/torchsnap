# Forcing Garbage Collection in a Tauri Webview

> **Status as of 2026-04-30 — superseded by a non-GC fix on macOS**
>
> The macOS-specific memory problem that motivated this research was
> resolved without forcing GC. ADR 0020 ("Size launcher webview to
> content and shrink on hide") sized the launcher window to its content
> bounds (~808×830) instead of the full monitor, then shrinks it to 1×1
> after hide. This turned the WebContent memory profile from a staircase
> into a sawtooth: average footprint dropped 145 MB → 55 MB and final
> idle 148 MB → 39 MB, with no need to invoke any GC API.
> *Implementation: `src-tauri/src/lib.rs`, `src-tauri/src/platform/macos/launcher_panel.rs`,
> `src-tauri/src/platform/fallback/launcher_panel.rs`.*
>
> Conclusions adopted:
> - JS-side cleanup discipline (unlisten every `listen()`, revoke Blob
>   URLs, null large refs) is still the cross-platform baseline.
>
> Conclusions deferred / not implemented:
> - Windows `TrySuspend()` / `MemoryUsageTargetLevel` — not yet wired up;
>   Torchsnap currently ships only macOS, so this remains a future
>   item if/when Windows support lands.
> - Linux `webkit_web_context_garbage_collect_javascript_objects()` —
>   same: deferred until Linux support.
> - `--js-flags=--expose-gc` / CDP `HeapProfiler.collectGarbage` — not
>   needed, the shrink-on-hide reclaim was sufficient.
> - Inspector-protocol-based `Heap.gc` — explicitly rejected as
>   non-production.
>
> The "destroy-on-hide / navigate-to-blank / DOM cleanup" alternatives
> noted in ADR 0020's consequences section remain available if the
> residual ~39 MB idle footprint ever becomes a concern.

## Motivation

Torchsnap uses a launcher-style UI: the webview is shown temporarily, then
hidden again. While the launcher is hidden, the webview should shed as much
memory as possible. This document catalogs available mechanisms for triggering
JavaScript garbage collection and reducing memory footprint in a Tauri webview,
per platform.

## Summary

There is no single, clean, cross-platform API for this. Each platform's webview
engine exposes different (and limited) mechanisms. Tauri and wry expose none of
them directly — every approach requires reaching below the wry abstraction layer
or relying on JS-side cleanup patterns.

macOS is the most constrained platform: WKWebView's process isolation blocks all
host-side GC triggers, and JSC has no `--expose-gc` equivalent for embedded
webviews. Windows (WebView2/V8) is the best-equipped. Linux (WebKitGTK) has a
dedicated C API function but it is "best effort."

## Recommended Approach for Launcher Hide/Show

The pattern is: **JS cleanup → hide window → platform-specific GC trigger**.

1. Before hiding, run a JS cleanup routine (cross-platform, see below).
2. Hide the window.
3. Fire the platform-specific GC mechanism if available.
4. On show, resume as needed (Windows `TrySuspend` auto-resumes on visibility).

## Platform Details

### macOS — WKWebView / JavaScriptCore

**From Rust (host side): No public API available.**

- `JSGarbageCollect(JSContextRef)` exists in the JavaScriptCore C API
  ([Apple docs][apple-jsgc]), but WKWebView does not expose the `JSContextRef`
  of its web content process. This is a deliberate multi-process isolation
  decision — web content runs in a separate `com.apple.WebKit.WebContent`
  process. There is a longstanding Radar (rdar://17680867) requesting Apple
  expose `JSContext` on WKWebView, which has gone unanswered.
- `GCController.collect()` exists inside WebKit internals and the inspector
  protocol calls it, but it is not exposed as a public ObjC/Swift API.
- Private APIs (e.g., `_garbageCollectNow` on internal WebKit objects) exist
  but would cause Mac App Store rejection. The [Apple Developer Forums
  thread][apple-forums] explicitly documents that there is no publicly
  supported way.

**From JavaScript: No trigger available.**

- JSC has no equivalent of V8's `--expose-gc` flag for embedded webviews.
- The `$vm.gc()` function used in WebKit layout tests is only accessible from
  the inspector console, not from normal page JS.

**Via WebKit Inspector Protocol:**

- The `Heap.gc` command ([protocol definition][webkit-heap-json]) can be sent
  over the WebKit Remote Debugging Protocol. This is what Playwright uses for
  `page.requestGC()` on WebKit ([Playwright issue #32278][playwright-gc]).
- Requires enabling `isInspectable = true` on the webview and connecting via
  WebSocket to the `webinspectord` daemon's Unix socket.
- Complex to set up from Tauri and not suitable for production use.

**Bottom line for macOS:** JS-side cleanup before hiding is the only viable
production path. The OS memory pressure system should eventually reclaim pages
from the hidden process, but there is no way to force it.

### Windows — WebView2 / V8

Best platform for GC control. Three mechanisms available:

**1. `window.gc()` via `--expose-gc` (simplest approach)**

Pass V8 flags through Tauri's `additionalBrowserArgs` config:

```json
{
  "tauri": {
    "windows": [{
      "additionalBrowserArgs": "--js-flags=--expose-gc"
    }]
  }
}
```

Then call `window.gc()` from JS for a synchronous full V8 GC cycle. Note:
when overriding `additionalBrowserArgs`, you must re-include the three default
flags wry normally sets (see [tauri#11144][tauri-11144]).

**2. Native WebView2 APIs (from Rust via COM interop)**

- **`TrySuspend()` / `Resume()`** (`ICoreWebView2_3`, SDK 1.0.774.44+):
  Pauses script timers/animations, minimizes CPU, tells the OS it can swap out
  renderer process pages. The most aggressive memory-reduction lever short of
  destroying the webview. Requires `IsVisible = false` first. Auto-resumes
  when the webview becomes visible. ([Microsoft docs][ms-trysuspend],
  [Rust bindings][webview2-sys-rs])

- **`MemoryUsageTargetLevel`** (`ICoreWebView2_6`): Set to
  `COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW` for webviews that should stay
  alive but use less memory. Causes the runtime to swap memory pages to disk.
  Does not directly trigger GC — it is a hint to the process memory manager.
  ([Microsoft spec][ms-memory-target])

**3. Chrome DevTools Protocol**

Call `HeapProfiler.collectGarbage` via
`ICoreWebView2::CallDevToolsProtocolMethod` with an empty params object `{}`.
This triggers a real Chromium GC cycle. Requires accessing the raw COM pointer
beneath wry's abstraction. ([CDP docs][cdp-heap], [WebView2 CDP
usage][ms-cdp])

### Linux — WebKitGTK / JavaScriptCore

**From Rust (host side): Public C API available.**

```c
webkit_web_context_garbage_collect_javascript_objects(WebKitWebContext *context);
```

This calls `WebProcess::garbageCollectJavaScriptObjects` inside the web
process. Not in the current `webkit2gtk` Rust crate bindings, but callable via
`glib` FFI. Caveats:

- It is "best effort" and not 100% reliable — the [WPE WebKit issue
  #1363][wpe-gc] contains cases where memory persisted despite forced GC.
- Developers report success calling it on a timer after applications return to
  idle.

**From JavaScript:** Same limitations as macOS — no `--expose-gc` equivalent.

**Via WebKit Inspector Protocol:**

- Same `Heap.gc` command as macOS. Enable with the environment variable
  `WEBKIT_INSPECTOR_SERVER=127.0.0.1:9222`.
  ([WebKit remote inspector docs][webkit-remote-inspector])

**JSC environment variables** (`JSC_gcMaxHeapSize`,
`JSC_criticalGCMemoryThreshold`, `JSC_minimumGCPauseMS`, etc.) can tune the
automatic collector's thresholds but do not expose a manual trigger. Set on the
process environment before launch. Full option list available via
`JSC_dumpOptions=3`.

## Platform Comparison

| Platform | Engine        | Host-side GC API                 | JS-side GC trigger                      | Protocol-based GC                  |
|----------|---------------|----------------------------------|-----------------------------------------|------------------------------------|
| macOS    | WKWebView/JSC | None (process isolation)         | None                                    | `Heap.gc` (debug, complex setup)   |
| Windows  | WebView2/V8   | `TrySuspend`, `MemoryTargetLvl` | `window.gc()` with `--expose-gc`        | `HeapProfiler.collectGarbage` CDP  |
| Linux    | WebKitGTK/JSC | `..._garbage_collect_...()` FFI  | None                                    | `Heap.gc` (requires env var)       |

## Cross-Platform Memory Reduction Strategies

Since macOS has no real GC trigger, aggressive JS-side cleanup is the most
reliable cross-platform approach.

### JavaScript-Side Cleanup

- **`URL.revokeObjectURL(url)`** — release Blob URLs immediately after use.
  WKWebView and WebKitGTK will not free the underlying Blob until the URL is
  revoked.
- **Null out large references** — `myLargeArray = null; myCanvas = null;`
  before navigating or after heavy operations.
- **Remove event listeners** via `removeEventListener` or `AbortController`.
- **Call unlisten for every Tauri `listen()`** — Tauri's
  `transformCallback` mechanism retains callbacks on `window` indefinitely. You
  must call the unlisten function returned by `listen()` to free it.
  ([tauri#13133][tauri-13133])
- **`FinalizationRegistry` / `WeakRef`** — useful for leak detection, not for
  triggering collection.

### Navigation-Based Flush

- **Navigate to `about:blank` and back** — destroys the JS heap of the
  previous page entirely. Nuclear option but works on all platforms. Resets all
  state.
- **`window.location.reload()`** — frees partial memory but the JS heap of the
  current page persists through reload.

### Tauri Resource Management

- Call the unlisten function for every `listen()` when the listener is no
  longer needed.
- Tauri 2.0's `on_page_load` with `PageLoadEvent::Started` can clear Rust-side
  resource tables on navigation ([tauri#10266][tauri-10266]).
- **Closing a `WebviewWindow`** (rather than hiding it) fully frees renderer
  process memory. Note: there is an open bug where closed windows on
  macOS/Windows do not always release the renderer process — verify with your
  Tauri version ([tauri#5397][tauri-5397]).

### Platform-Specific Optimizations

- **Windows:** Call `TrySuspend()` when the webview is hidden/minimized. This
  is the highest-leverage single action on Windows.
- **Linux:** Call `webkit_web_context_garbage_collect_javascript_objects()` from
  Rust via FFI on idle (e.g., after 30s of inactivity).

## References

[apple-jsgc]: https://developer.apple.com/documentation/javascriptcore/1451393-jsgarbagecollect
[apple-forums]: https://developer.apple.com/forums/thread/116498
[webkit-heap-json]: https://github.com/WebKit/webkit/blob/main/Source/JavaScriptCore/inspector/protocol/Heap.json
[playwright-gc]: https://github.com/microsoft/playwright/issues/32278
[tauri-11144]: https://github.com/tauri-apps/tauri/issues/11144
[ms-trysuspend]: https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2_3
[webview2-sys-rs]: https://docs.rs/webview2-sys/latest/webview2_sys/trait.ICoreWebView2_3.html
[ms-memory-target]: https://github.com/MicrosoftEdge/WebView2Feedback/blob/main/specs/MemoryUsageTargetLevel.md
[cdp-heap]: https://chromedevtools.github.io/devtools-protocol/tot/HeapProfiler/
[ms-cdp]: https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/chromium-devtools-protocol
[wpe-gc]: https://github.com/WebPlatformForEmbedded/WPEWebKit/issues/1363
[webkit-remote-inspector]: https://trac.webkit.org/wiki/RemoteInspectorGTKandWPE
[tauri-13133]: https://github.com/tauri-apps/tauri/issues/13133
[tauri-10266]: https://github.com/tauri-apps/tauri/issues/10266
[tauri-5397]: https://github.com/tauri-apps/tauri/issues/5397
