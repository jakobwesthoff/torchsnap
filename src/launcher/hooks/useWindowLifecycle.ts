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
import { command } from "../../lib/command";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

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
    // Route through the Rust-side `launcher_hide` command rather than
    // `appWindow.hide()` so that the window is also shrunk to 1×1 px,
    // prompting WebKit to release its backing stores. See ADR 0020.
    command("launcher_hide");
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
