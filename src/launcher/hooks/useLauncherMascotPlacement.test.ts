// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { getMascotAnchor } from "../../mascot";
import { placeMascot } from "../placement";
import { useLauncherMascotPlacement } from "./useLauncherMascotPlacement";

describe("useLauncherMascotPlacement", () => {
  it("places nothing while the mascot is off", () => {
    const { result } = renderHook(() => useLauncherMascotPlacement("original", "off"));
    expect(result.current).toBeNull();
  });

  it.each(["center", "sidekick"] as const)(
    "places the variant by its anchor in %s mode",
    (mode) => {
      const { groundAnchor, boxLeft } = getMascotAnchor("original");
      const { result } = renderHook(() => useLauncherMascotPlacement("original", mode));
      expect(result.current).toEqual(placeMascot(mode, groundAnchor, boxLeft));
    },
  );

  it("follows a change of variant", () => {
    const { result, rerender } = renderHook(
      ({ variant }) => useLauncherMascotPlacement(variant, "center"),
      {
        initialProps: { variant: "original" },
      },
    );
    const first = result.current;
    rerender({ variant: "torchbearer" });
    const { groundAnchor, boxLeft } = getMascotAnchor("torchbearer");
    expect(result.current).toEqual(placeMascot("center", groundAnchor, boxLeft));
    expect(result.current).not.toEqual(first);
  });
});
