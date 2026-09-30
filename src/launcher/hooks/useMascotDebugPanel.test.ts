// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useMascotDebugPanel } from "./useMascotDebugPanel";

describe("useMascotDebugPanel", () => {
  it("starts closed", () => {
    const { result } = renderHook(() => useMascotDebugPanel());
    expect(result.current.open).toBe(false);
  });

  it("opens on show and stays open until closed", () => {
    const { result } = renderHook(() => useMascotDebugPanel());
    act(() => result.current.show());
    act(() => result.current.show());
    expect(result.current.open).toBe(true);
    act(() => result.current.close());
    expect(result.current.open).toBe(false);
  });

  it("keeps its callbacks stable across renders", () => {
    const { result, rerender } = renderHook(() => useMascotDebugPanel());
    const { show, close } = result.current;
    act(() => show());
    rerender();
    expect(result.current.show).toBe(show);
    expect(result.current.close).toBe(close);
  });
});
