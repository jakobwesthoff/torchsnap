// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it } from "vitest";
import type { ActionSlot } from "./types";

// `ActionSlot` mirrors the Rust `Slot` enum; the type check is the
// real test.
describe("ActionSlot", () => {
  it("names every slot the host serializes", () => {
    const slots: ActionSlot[] = [
      "primary",
      "secondary",
      "copy",
      "reveal",
      "delete",
      "openSettings",
    ];
    expect(slots).toHaveLength(6);
  });
});
