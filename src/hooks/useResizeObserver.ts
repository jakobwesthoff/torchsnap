// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { useEffect, type RefObject } from "react";

/**
 * Observe an element's size via `ResizeObserver`.
 *
 * When `callback` is provided, attaches an observer to the element
 * referenced by `ref` and invokes `callback` on every size change.
 * When `callback` is `undefined`, no observer is created.
 *
 * Cleans up automatically on unmount or when `callback` changes to
 * `undefined`.
 */
export function useResizeObserver(
  ref: RefObject<HTMLElement | null>,
  callback: ((entry: ResizeObserverEntry) => void) | undefined,
) {
  useEffect(() => {
    if (!callback || !ref.current) return;

    const el = ref.current;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        callback(entry);
      }
    });

    observer.observe(el);
    return () => observer.disconnect();
  }, [ref, callback]);
}
