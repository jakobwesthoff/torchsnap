// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeAll, describe, expect, it, vi } from "vitest";
import {
  MockGadgetContextProvider,
  setupSdkGlobalsForTesting,
} from "@torchsnap/gadget-sdk/testing";
import { CalculatorInline } from "./CalculatorInline";

beforeAll(() => setupSdkGlobalsForTesting());

const RESULT = { expression: "2+2", result: "4", resultType: "number" };

function renderInline({
  data = RESULT as unknown,
  selected = true,
  sendMessage = vi.fn().mockResolvedValue(null),
  dismiss = vi.fn(),
  onExecute = vi.fn(),
  logError = vi.fn(),
} = {}) {
  const logger = {
    trace: vi.fn(),
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: logError,
    spanStart: vi.fn(() => 0),
    spanEnd: vi.fn(),
  };
  const view = render(
    <MockGadgetContextProvider
      runtime={{ sendMessage, logger }}
      launcher={{ dismiss, onExecute }}
    >
      <CalculatorInline data={data} query="2+2" matchedPrefix="" selected={selected} />
    </MockGadgetContextProvider>,
  );
  return { ...view, sendMessage, dismiss, onExecute, logError };
}

describe("CalculatorInline", () => {
  it("shows the result and the expression it came from", () => {
    renderInline({ selected: false });

    expect(screen.getByText("4")).toBeInTheDocument();
    expect(screen.getByText("2+2")).toBeInTheDocument();
  });

  it("renders nothing without a result", () => {
    const { container } = renderInline({ data: null, selected: false });

    expect(container).toBeEmptyDOMElement();
  });

  it("copies the result through the backend and closes the launcher on Enter", async () => {
    const { sendMessage, dismiss, onExecute } = renderInline();

    fireEvent.keyDown(document, { key: "Enter" });

    expect(sendMessage).toHaveBeenCalledWith("copy", RESULT);
    await waitFor(() => expect(dismiss).toHaveBeenCalledOnce());
    expect(onExecute).not.toHaveBeenCalled();
  });

  it("ignores Enter while the inline slot is not selected", () => {
    const { sendMessage, dismiss } = renderInline({ selected: false });

    fireEvent.keyDown(document, { key: "Enter" });

    expect(sendMessage).not.toHaveBeenCalled();
    expect(dismiss).not.toHaveBeenCalled();
  });

  it("keeps the launcher open and logs when copying fails", async () => {
    const sendMessage = vi.fn().mockRejectedValue(new Error("clipboard denied"));
    const { dismiss, logError } = renderInline({ sendMessage });

    fireEvent.keyDown(document, { key: "Enter" });

    await waitFor(() => expect(logError).toHaveBeenCalled());
    expect(dismiss).not.toHaveBeenCalled();
  });
});
