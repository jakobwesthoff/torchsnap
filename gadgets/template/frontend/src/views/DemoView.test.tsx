// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeAll, describe, expect, it, vi } from "vitest";
import {
  MockGadgetContextProvider,
  setupSdkGlobalsForTesting,
} from "@torchsnap/gadget-sdk/testing";
import { DemoView } from "./DemoView";

beforeAll(() => setupSdkGlobalsForTesting());

describe("DemoView", () => {
  it("reports a failed action in the launcher footer", async () => {
    const sendMessage = vi.fn(async (method: string) => {
      if (method === "no-such-method") throw "unknown method `no-such-method`";
      return method === "current-greeting" ? { greeting: "hi" } : { enable_count: 1 };
    });
    const showError = vi.fn();
    render(
      <MockGadgetContextProvider
        runtime={{ sendMessage: sendMessage as never }}
        launcher={{ showError }}
      >
        <DemoView query="" matchedPrefix="tpl:" results={[]} />
      </MockGadgetContextProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Fail an action" }));

    await waitFor(() =>
      expect(showError).toHaveBeenCalledWith(
        "Demo action failed: unknown method `no-such-method`",
      ),
    );
  });
});
