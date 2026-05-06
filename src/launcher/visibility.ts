// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Linux-only launcher visibility choreography.
 *
 * WebKitGTK does not composite frames for an unmapped `GtkWindow`.
 * On `win.hide()` the swapchain retains whatever frame was last
 * produced before the window was unmapped, and the next `win.show()`
 * presents that stale frame until a new one is composed. Without
 * intervention, the launcher flashes the previous session's query,
 * results, and mascot on every re-show.
 *
 * We cannot force a paint while hidden. What we can do is choose
 * the frame that *will* be stale: immediately before unmapping, we
 * set `#root`'s visibility to `hidden` and wait one frame so
 * WebKitGTK composites a blank buffer. The next re-show then
 * presents a blank window, and we only restore visibility once the
 * post-show DOM has had a chance to paint — so the content appears
 * "fresh" instead of flashing.
 *
 * Every dismiss path in the launcher — tray blur, ESC, Escape
 * keybinding, gadget-execute completion, Rust-initiated
 * `launcher-dismiss-requested` events — must route through
 * `dismissLauncher()` below. A direct `command("launcher_hide")`
 * call skips the blanking step and reintroduces the flash.
 *
 * # Concurrency model
 *
 * Two module-level booleans coordinate dismiss and restore to make
 * interleaved hide/show events safe:
 *
 * - `isHiding`: set by `dismissLauncher` at entry, cleared by
 *   `restoreVisibility` at entry.
 * - `isShowing`: set by `restoreVisibility` at entry, cleared by
 *   `dismissLauncher` at entry.
 *
 * The two flags are mutually exclusive and flipped together — each
 * operation claims its direction and aborts the other's in-flight
 * counterpart. The entry guards make calls idempotent against
 * their own kind (two dismisses in a row, two restores in a row).
 * The intra-step `if (!isHiding) return` / `if (!isShowing) return`
 * lines make an in-flight operation abandon when a counterpart
 * wins the race during a `requestAnimationFrame` wait.
 *
 * A `tauri://blur` fired as a side effect of `win.hide()` will
 * invoke `dismissLauncher` a second time; the entry guard (already
 * hiding) short-circuits so no extra rAFs accumulate on the
 * hidden-window queue. That zombie rAF queue is what caused
 * `launcher_hide` to fire after subsequent shows — the guards here
 * are what prevent it.
 */

import { platform } from "@tauri-apps/plugin-os";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { command } from "../lib/command";

const win = getCurrentWebviewWindow();

// Runtime platform detection is evaluated once — `platform()` returns
// a constant for a given process, so caching the result lets the hot
// paths stay synchronous.
const isLinux = platform() === "linux";

// See the module comment for the role of these flags. They are
// only consulted on Linux; the macOS / Windows paths do not touch
// `#root` and do not need coordination.
let isHiding = false;
let isShowing = false;

function nextFrame(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => resolve());
  });
}

/**
 * Dismiss the launcher through the WebKitGTK-safe path on Linux,
 * or fall through to a direct `launcher_hide` on every other
 * platform. This is the one and only dismiss entry point for the
 * frontend.
 */
export async function dismissLauncher(): Promise<void> {
  if (isLinux) {
    if (isHiding) {
      return;
    }
    isHiding = true;
    isShowing = false;

    const root = document.getElementById("root");
    if (root !== null) {
      root.style.visibility = "hidden";
      await nextFrame();
      if (!isHiding) {
        return;
      }
      await nextFrame();
      if (!isHiding) {
        return;
      }
    }
  }

  await command("launcher_hide");
}

export async function setupLauncherVisibilityChoreography(): Promise<void> {
  // The blanking dance below exists only to compensate for
  // WebKitGTK's unmap-drops-compositing behaviour. On macOS,
  // `WKWebView` keeps the swapchain alive across hide/show so the
  // first frame post-reveal already matches the current DOM —
  // no dance is needed there, and the listeners below would
  // have no events to receive because Rust only emits the
  // corresponding events on Linux.
  if (!isLinux) {
    return;
  }

  const root = document.getElementById("root");
  if (root === null) {
    return;
  }

  // Externally-initiated dismisses (hotkey toggle, Control API
  // `dismiss`). Only emitted by Rust on Linux — see
  // `request_launcher_dismiss` in `src-tauri/src/lib.rs`.
  // Frontend-initiated dismisses call `dismissLauncher()` directly
  // and do not rely on this event.
  await win.listen("launcher-dismiss-requested", () => {
    void dismissLauncher();
  });

  // Restore `#root` to visible after the window has been shown and
  // given WebKit time to compose a fresh frame from the current DOM
  // (which by now has a cleared query, empty results, and a re-rolled
  // mascot). The two rAFs bracket one full presentation cycle.
  //
  // Listeners fire on two signals:
  //   - `launcher-shown`: emitted by Rust unconditionally after
  //     `PlatformLauncherPanel::show` succeeds. Reliable trigger
  //     regardless of whether the compositor actually gives the
  //     window keyboard focus (Mutter's focus-stealing prevention
  //     may deny `set_focus` on Wayland, so `tauri://focus` is
  //     not a reliable "the launcher is visible" signal by itself).
  //   - `tauri://focus`: kept as a secondary trigger so the
  //     restore still happens on platforms or scenarios where
  //     the Rust-side event is missed. Re-running idempotently
  //     on both is harmless thanks to the `isShowing` guard.
  const restoreVisibility = async (): Promise<void> => {
    if (isShowing) {
      return;
    }
    isShowing = true;
    isHiding = false;

    await nextFrame();
    if (!isShowing) {
      return;
    }
    await nextFrame();
    if (!isShowing) {
      return;
    }

    root.style.visibility = "visible";
  };

  await win.listen("launcher-shown", () => {
    void restoreVisibility();
  });
  await win.listen("tauri://focus", () => {
    void restoreVisibility();
  });
}
