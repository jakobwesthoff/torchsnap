// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Emacs/readline key bindings for text inputs.
 *
 * macOS natively supports these Ctrl+key bindings in NSTextView via
 * the text input system, but WKWebView (used by Tauri) does not
 * forward them to input elements. This hook re-implements the most
 * commonly used ones so they work in the launcher search input.
 *
 * Returns `{ onKeyDown }` props to spread onto an `<input>` element.
 *
 * Supported bindings:
 * - **Ctrl+W** — delete word backward
 * - **Ctrl+U** — delete from cursor to beginning of line
 * - **Ctrl+K** — delete from cursor to end of line
 * - **Ctrl+A** — move cursor to beginning of line
 * - **Ctrl+E** — move cursor to end of line
 */

import { useCallback, useRef, type RefObject } from "react";

export function useEmacsBindings(
  inputRef: RefObject<HTMLInputElement | null>,
  setValue: (v: string) => void,
) {
  // Store setValue in a ref so the returned onKeyDown callback is
  // referentially stable regardless of whether the caller passes a
  // new function identity on each render.
  const setValueRef = useRef(setValue);

  // Render-time ref mutation ("latest-ref" pattern). The rule exists
  // because concurrent mode can replay renders, making ref writes during
  // render a potentially unsafe side effect. That concern does not apply
  // here: the only consumer is onKeyDown, which fires from keyboard
  // events — always well after the render has committed. Wrapping this
  // in a useEffect would add overhead for no practical benefit.
  // eslint-disable-next-line react-hooks/refs
  setValueRef.current = setValue;

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (!e.ctrlKey || e.metaKey) {
        return;
      }

      const input = inputRef.current;
      if (!input) {
        return;
      }

      const pos = input.selectionStart ?? input.value.length;
      const before = input.value.slice(0, pos);
      const after = input.value.slice(pos);

      switch (e.key) {
        // -------------------------------------------------------
        // Ctrl+W — delete word backward
        //
        // Consumes trailing whitespace first, then the preceding
        // word. If only whitespace remains, clears it entirely.
        //   " foo bar  baz  |"   → " foo bar  |"
        //   " foo bar  |"        → " foo |"
        //   " foo |"             → " |"
        //   " |"                 → "|"
        // -------------------------------------------------------
        case "w": {
          e.preventDefault();
          // Try removing a word + its trailing whitespace. If that
          // doesn't match (only whitespace left), clear all of it.
          let trimmed = before.replace(/\S+\s*$/, "");
          if (trimmed === before) {
            trimmed = "";
          }
          setValueRef.current(trimmed + after);
          requestAnimationFrame(() => {
            input.setSelectionRange(trimmed.length, trimmed.length);
          });
          break;
        }

        // -------------------------------------------------------
        // Ctrl+U — delete from cursor to beginning of line
        //   "foo bar|baz"  → "|baz"
        // -------------------------------------------------------
        case "u": {
          e.preventDefault();
          setValueRef.current(after);
          requestAnimationFrame(() => {
            input.setSelectionRange(0, 0);
          });
          break;
        }

        // -------------------------------------------------------
        // Ctrl+K — delete from cursor to end of line
        //   "foo|bar baz"  → "foo|"
        // -------------------------------------------------------
        case "k": {
          e.preventDefault();
          setValueRef.current(before);
          requestAnimationFrame(() => {
            input.setSelectionRange(before.length, before.length);
          });
          break;
        }

        // -------------------------------------------------------
        // Ctrl+A — move cursor to beginning of line
        // -------------------------------------------------------
        case "a":
          e.preventDefault();
          input.setSelectionRange(0, 0);
          break;

        // -------------------------------------------------------
        // Ctrl+E — move cursor to end of line
        // -------------------------------------------------------
        case "e":
          e.preventDefault();
          input.setSelectionRange(input.value.length, input.value.length);
          break;
      }
    },
    [inputRef],
  );

  return { onKeyDown };
}
