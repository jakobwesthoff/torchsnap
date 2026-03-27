// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Manages a windowed view over a result list.
 *
 * Instead of rendering all results in a scrollable container, only
 * `pageSize` rows are rendered at a time. The visible window is
 * driven by two independent inputs:
 *
 * - **Keyboard navigation** changes `selectedIndex` externally.
 *   The hook adjusts `windowStart` to keep the selection visible.
 *
 * - **Mouse wheel** shifts `windowStart` directly and co-shifts
 *   `selectedIndex` by the same delta so the highlight stays at
 *   the same visual position. The row that ends up under the
 *   stationary cursor then receives a `mouseenter` event from the
 *   browser, which updates the selection through the existing
 *   hover-selection path.
 */

import { useEffect, useRef, type RefObject } from "react";

interface UseWindowedListParams {
  selectedIndex: number;
  setSelectedIndex: (index: number) => void;
  resultCount: number;
  pageSize: number;
  wheelRef: RefObject<HTMLDivElement | null>;
}

export function useWindowedList({
  selectedIndex,
  setSelectedIndex,
  resultCount,
  pageSize,
  wheelRef,
}: UseWindowedListParams): { windowStart: number } {
  // =========================================================
  // Window Position (ref-based, adjusted synchronously)
  //
  // Using a ref instead of state avoids an extra render cycle.
  // Every caller that changes `selectedIndex` or triggers a
  // wheel event already causes a re-render, during which we
  // read and adjust the ref synchronously.
  // =========================================================

  const windowStartRef = useRef(0);

  // Reset window position when the result set changes (new query).
  const prevResultCountRef = useRef(resultCount);
  if (prevResultCountRef.current !== resultCount) {
    prevResultCountRef.current = resultCount;
    windowStartRef.current = 0;
  }

  // =========================================================
  // Keyboard Follow
  //
  // When keyboard navigation pushes `selectedIndex` outside the
  // visible window, adjust `windowStart` to bring it back into
  // view with minimal movement. This runs synchronously during
  // render — no intermediate frame with a stale window.
  // =========================================================

  let ws = windowStartRef.current;
  if (selectedIndex < ws) {
    ws = selectedIndex;
  } else if (selectedIndex >= ws + pageSize) {
    ws = selectedIndex - pageSize + 1;
  }

  // Clamp to valid range (handles edge cases where resultCount
  // shrinks after the window was positioned further down).
  const maxStart = Math.max(0, resultCount - pageSize);
  ws = Math.max(0, Math.min(ws, maxStart));
  windowStartRef.current = ws;

  // =========================================================
  // Mouse Wheel
  //
  // Shifts the window and co-shifts `selectedIndex` by the same
  // delta so the highlight stays at the same visual row position.
  // Without the co-shift, there would be a single frame where the
  // window moved but the selection hadn't updated yet, causing the
  // highlight to visually jump before `mouseenter` corrects it.
  //
  // The browser should then fire `mouseenter` on the row that
  // appears under the stationary cursor after React re-renders
  // with the new slice, which updates the selection via the
  // existing hover-selection path in `ResultRow`.
  //
  // NOTE: If browsers don't reliably fire `mouseenter` when DOM
  // content shifts under a stationary cursor, the fallback is to
  // use `document.elementFromPoint(e.clientX, e.clientY)` after
  // the state update to manually determine which row is under the
  // pointer and call `onSelectIndex` with the computed index.
  //
  // The listener must be registered as non-passive so that
  // `preventDefault()` works — React's synthetic `onWheel` is
  // passive in modern browsers.
  // =========================================================

  // Stable ref for the wheel handler to read current values
  // without re-registering the listener on every render.
  const stateRef = useRef({ selectedIndex, resultCount, pageSize });
  stateRef.current = { selectedIndex, resultCount, pageSize };

  useEffect(() => {
    const el = wheelRef.current;
    if (!el) return;

    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const delta = Math.sign(e.deltaY);
      if (delta === 0) return;

      const { selectedIndex: sel, resultCount: count, pageSize: ps } = stateRef.current;
      const currentWs = windowStartRef.current;
      const max = Math.max(0, count - ps);
      const newWs = Math.max(0, Math.min(currentWs + delta, max));

      if (newWs === currentWs) return;

      // Co-shift selection so the highlight stays at the same
      // visual position within the window.
      const visualOffset = sel - currentWs;
      const newSel = Math.max(0, Math.min(newWs + visualOffset, count - 1));

      windowStartRef.current = newWs;
      setSelectedIndex(newSel);
    };

    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [wheelRef, setSelectedIndex]);

  return { windowStart: windowStartRef.current };
}
