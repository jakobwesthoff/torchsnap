// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it } from "vitest";
import type { ActionId } from "./types";

// `ActionId` mirrors the Rust enum; the type check is the real test.
describe("ActionId", () => {
  it("covers openSettings, which gadgets put on configuration entries", () => {
    const id: ActionId = { type: "openSettings" };
    expect(id).toEqual({ type: "openSettings" });
  });
});
