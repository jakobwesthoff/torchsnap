// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it } from "vitest";
import { accelToDisplayParts } from "./accelerator";
import { formatKey, formatModifier } from "./platform";

describe("accelToDisplayParts", () => {
  it("formats both spellings of the primary modifier as Meta", () => {
    expect(accelToDisplayParts("CmdOrCtrl+K")).toEqual([formatModifier("Meta"), formatKey("K")]);
    expect(accelToDisplayParts("CommandOrControl+K")).toEqual([
      formatModifier("Meta"),
      formatKey("K"),
    ]);
  });

  it("formats Shift and Alt and keeps the order", () => {
    expect(accelToDisplayParts("CmdOrCtrl+Alt+Shift+Space")).toEqual([
      formatModifier("Meta"),
      formatModifier("Alt"),
      formatModifier("Shift"),
      formatKey("Space"),
    ]);
  });

  it("formats a lone key", () => {
    expect(accelToDisplayParts("F5")).toEqual([formatKey("F5")]);
  });
});
