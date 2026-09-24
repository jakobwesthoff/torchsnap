// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { accelToDisplayParts } from "../keybindings/accelerator";
import { ShortcutKeys } from "./ShortcutKeys";

describe("ShortcutKeys", () => {
  it("renders one keycap per part of the shortcut", () => {
    const { container } = render(<ShortcutKeys shortcut="CmdOrCtrl+Shift+Space" />);
    const caps = Array.from(container.querySelectorAll("kbd")).map((k) => k.textContent);
    expect(caps).toEqual(accelToDisplayParts("CmdOrCtrl+Shift+Space"));
  });
});
