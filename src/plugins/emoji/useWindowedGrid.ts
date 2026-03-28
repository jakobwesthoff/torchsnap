// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Manages a windowed view over a grid of results.
 *
 * Parallel to `useWindowedList` but operates on rows of columns.
 * Only `visibleRows` rows are rendered at a time. The window shifts
 * via keyboard navigation (selection drives window position) or
 * mouse wheel (window shifts independently, hover updates selection
 * through the existing mouseenter path).
 */

import { useCallback, useEffect, useRef, useState } from "react";

interface UseWindowedGridParams {
  selectedIndex: number;
  setSelectedIndex: (index: number) => void;
  resultCount: number;
  columns: number;
  visibleRows: number;
}

interface UseWindowedGridResult {
  /** First visible row index (0-based). Multiply by columns to get
   *  the first visible item index. */
  windowStartRow: number;
  /** Callback ref to attach to the grid container element. */
  wheelRef: (el: HTMLDivElement | null) => void;
}

export function useWindowedGrid({
  selectedIndex,
  setSelectedIndex,
  resultCount,
  columns,
  visibleRows,
}: UseWindowedGridParams): UseWindowedGridResult {
  const windowStartRowRef = useRef(0);

  // Reset window position when the result set changes (new query).
  const prevResultCountRef = useRef(resultCount);
  if (prevResultCountRef.current !== resultCount) {
    prevResultCountRef.current = resultCount;
    windowStartRowRef.current = 0;
  }

  // -------------------------------------------------------
  // Keyboard follow — runs synchronously during render.
  //
  // When the selected index moves to a row outside the visible
  // window, shift the window to keep it visible.
  // -------------------------------------------------------
  const selectedRow = Math.floor(selectedIndex / columns);
  const totalRows = Math.ceil(resultCount / columns);
  const maxStartRow = Math.max(0, totalRows - visibleRows);

  let wsRow = windowStartRowRef.current;
  if (selectedRow < wsRow) {
    wsRow = selectedRow;
  } else if (selectedRow >= wsRow + visibleRows) {
    wsRow = selectedRow - visibleRows + 1;
  }
  wsRow = Math.max(0, Math.min(wsRow, maxStartRow));
  windowStartRowRef.current = wsRow;

  // -------------------------------------------------------
  // Mouse wheel — uses a callback ref so the listener is
  // attached exactly when the target element mounts, even
  // if it is conditionally rendered.
  // -------------------------------------------------------
  const [wheelEl, setWheelEl] = useState<HTMLDivElement | null>(null);
  const wheelRef = useCallback((el: HTMLDivElement | null) => {
    setWheelEl(el);
  }, []);

  const stateRef = useRef({ selectedIndex, resultCount, columns, visibleRows });
  stateRef.current = { selectedIndex, resultCount, columns, visibleRows };

  useEffect(() => {
    if (!wheelEl) return;

    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const delta = Math.sign(e.deltaY);
      if (delta === 0) return;

      const {
        selectedIndex: sel,
        resultCount: count,
        columns: cols,
        visibleRows: vr,
      } = stateRef.current;
      const total = Math.ceil(count / cols);
      const maxStart = Math.max(0, total - vr);
      const currentWsRow = windowStartRowRef.current;
      const newWsRow = Math.max(0, Math.min(currentWsRow + delta, maxStart));

      if (newWsRow === currentWsRow) return;

      // Keep the selection at the same visual position within
      // the grid by shifting it by one row.
      const selRow = Math.floor(sel / cols);
      const selCol = sel % cols;
      const newSelRow = selRow + (newWsRow - currentWsRow);
      const newSel = Math.max(0, Math.min(newSelRow * cols + selCol, count - 1));

      windowStartRowRef.current = newWsRow;
      setSelectedIndex(newSel);
    };

    wheelEl.addEventListener("wheel", onWheel, { passive: false });
    return () => wheelEl.removeEventListener("wheel", onWheel);
  }, [wheelEl, setSelectedIndex]);

  return { windowStartRow: windowStartRowRef.current, wheelRef };
}
