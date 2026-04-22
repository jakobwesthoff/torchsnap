// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Manages the launcher window's show/hide lifecycle.
 *
 * Listens for Tauri focus/blur events to auto-focus the search input
 * on show and dismiss the launcher on blur (click outside).
 */

import { useCallback, useEffect, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { dismissLauncher } from "../visibility";

const appWindow = getCurrentWebviewWindow();

interface UseWindowLifecycleParams {
  inputRef: React.RefObject<HTMLInputElement | null>;
  mouseActiveRef: React.RefObject<boolean>;
  resetState: () => void;
}

export function useWindowLifecycle({
  inputRef,
  mouseActiveRef,
  resetState,
}: UseWindowLifecycleParams) {
  const [, setVisible] = useState(false);

  const dismiss = useCallback(() => {
    setVisible(false);
    resetState();
    // Always go through `dismissLauncher()` — on Linux it blanks
    // `#root` and waits one frame before invoking `launcher_hide`
    // so the swapchain's last composited buffer is empty rather
    // than a screenshot of the previous session's UI state (see
    // `src/launcher/visibility.ts`). On macOS / Windows it falls
    // straight through to the Rust command, which also shrinks
    // the window to 1×1 so WebKit releases its backing stores
    // (ADR 0020).
    void dismissLauncher();
  }, [resetState]);

  useEffect(() => {
    const unlistenFocus = appWindow.listen("tauri://focus", () => {
      setVisible(true);
      mouseActiveRef.current = false;
      requestAnimationFrame(() => inputRef.current?.focus());
    });

    const unlistenBlur = appWindow.listen("tauri://blur", () => {
      dismiss();
    });

    return () => {
      unlistenFocus.then((f) => f());
      unlistenBlur.then((f) => f());
    };
  }, [dismiss, inputRef, mouseActiveRef]);

  return { hide: dismiss, dismiss };
}
