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

import { useCallback, useEffect, useRef, useState } from "react";

interface UseWindowedListParams {
  selectedIndex: number;
  setSelectedIndex: (index: number) => void;
  resultCount: number;
  pageSize: number;
}

export function useWindowedList({
  selectedIndex,
  setSelectedIndex,
  resultCount,
  pageSize,
}: UseWindowedListParams): {
  windowStart: number;
  wheelRef: (el: HTMLDivElement | null) => void;
} {
  // =========================================================
  // Window Position (ref-based, adjusted synchronously)
  //
  // Using a ref instead of state avoids an extra render cycle.
  // Every caller that changes `selectedIndex` or triggers a
  // wheel event already causes a re-render, during which we
  // read and adjust the ref synchronously.
  // =========================================================

  const windowStartRef = useRef(0);

  // ESLINT: windowStartRef is read and written during render
  // intentionally — the windowing position is a deterministic
  // function of selectedIndex, resultCount, and pageSize, computed
  // synchronously to avoid an extra render cycle. Using state
  // would cause a double render on every keystroke. The concurrent
  // rendering concern (abandoned renders writing to refs) is
  // harmless here because the computation is pure: any
  // re-execution with the same inputs produces the same result.
  /* eslint-disable react-hooks/refs */

  // Window position is NOT reset on resultCount changes — with
  // incremental search results merging in, the window should stay
  // stable. The selection reset (triggered by query change in
  // Launcher.tsx) will naturally bring windowStart back to 0 for
  // new queries via the keyboard-follow logic below.

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

  /* eslint-enable react-hooks/refs */

  // =========================================================
  // Mouse Wheel
  //
  // Uses a callback ref so the wheel listener is attached
  // exactly when the target element mounts — even if it is
  // conditionally rendered (e.g. hidden while the list is
  // empty). When the element unmounts the effect cleans up.
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

  // Callback ref: React calls this when the element mounts or
  // unmounts, updating `wheelEl` state and triggering the effect.
  const [wheelEl, setWheelEl] = useState<HTMLDivElement | null>(null);
  const wheelRef = useCallback((el: HTMLDivElement | null) => {
    setWheelEl(el);
  }, []);

  // Stable ref for the wheel handler to read current values
  // without re-registering the listener on every render.
  //
  // ESLINT: This ref is only read inside the wheel event handler,
  // which cannot fire during render. Direct assignment during render
  // guarantees the ref is current before any post-commit event — a
  // useEffect wrapper would leave a gap where a wheel event could
  // read stale values.
  const stateRef = useRef({ selectedIndex, resultCount, pageSize });
  // eslint-disable-next-line react-hooks/refs
  stateRef.current = { selectedIndex, resultCount, pageSize };

  useEffect(() => {
    if (!wheelEl) return;

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

    wheelEl.addEventListener("wheel", onWheel, { passive: false });
    return () => wheelEl.removeEventListener("wheel", onWheel);
  }, [wheelEl, setSelectedIndex]);

  // ESLINT: windowStartRef was computed synchronously above during this
  // render pass — reading it here simply returns the value we just wrote.
  // eslint-disable-next-line react-hooks/refs
  return { windowStart: windowStartRef.current, wheelRef };
}
