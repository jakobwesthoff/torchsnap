// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Action Error
//
// The error of the last failed launcher action, shown in the
// footer in place of the hints (ADR 62). Actions are entry
// executes, `onExecute` and `openSettings` from gadget views,
// and a view's own failures reported through `showError`.
//
// The error stays until the user does something else: the next
// action, a key press, a query or selection change (the reset
// key), or hiding the launcher (`clearError`).
// =========================================================

import { useCallback, useEffect, useState } from "react";

/** Keys that only modify another key; pressing one alone is not a
 *  new interaction. */
const MODIFIER_KEYS = new Set(["Meta", "Control", "Alt", "Shift", "CapsLock", "Fn"]);

/** `<lead> failed: <message>`, with the message of an `Error` or the
 *  string a rejected Tauri command carries. */
export function actionErrorMessage(lead: string, error: unknown): string {
  const detail = error instanceof Error ? error.message : String(error);
  return `${lead} failed: ${detail}`;
}

export interface ActionErrorState {
  error: string | null;
  /** Show a message as given. */
  showError: (message: string) => void;
  clearError: () => void;
  /** Run an action, clearing the shown error first. A rejection shows
   *  `<lead> failed: <message>` and is passed on to the caller. */
  runAction: <T>(lead: string, action: () => Promise<T>) => Promise<T>;
}

/**
 * @param resetKey Clears the error whenever it changes. The launcher
 *   builds it from the query, the selection and the active view.
 */
export function useActionError(resetKey: string): ActionErrorState {
  const [error, setError] = useState<string | null>(null);

  // Adjusting state during render is React's prescribed way to reset
  // state when an input changes; React re-runs the component at once.
  const [prevResetKey, setPrevResetKey] = useState(resetKey);
  if (prevResetKey !== resetKey) {
    setPrevResetKey(resetKey);
    setError(null);
  }

  useEffect(() => {
    if (error === null) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (!MODIFIER_KEYS.has(e.key)) setError(null);
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [error]);

  const showError = useCallback((message: string) => setError(message), []);
  const clearError = useCallback(() => setError(null), []);

  const runAction = useCallback(async <T>(lead: string, action: () => Promise<T>) => {
    setError(null);
    try {
      return await action();
    } catch (e) {
      setError(actionErrorMessage(lead, e));
      throw e;
    }
  }, []);

  return { error, showError, clearError, runAction };
}
