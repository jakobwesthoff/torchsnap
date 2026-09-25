// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeAll, describe, expect, it, vi } from "vitest";
import type { SourcedEntry } from "@torchsnap/gadget-sdk";
import {
  MockGadgetContextProvider,
  setupSdkGlobalsForTesting,
} from "@torchsnap/gadget-sdk/testing";
import { CalculatorView } from "./CalculatorView";

beforeAll(() => setupSdkGlobalsForTesting());

const EVALUATED = { expression: "6*7", result: "42", resultType: "number" };

function historyEntry(id: string, expression: string, result: string): SourcedEntry {
  return {
    id,
    title: expression,
    subtitle: result,
    icon: null,
    score: 0,
    titlePositions: [],
    subtitlePositions: [],
    source: "calculator",
    actions: [],
  };
}

const HISTORY = [historyEntry("1", "1+1", "2"), historyEntry("2", "10/4", "2.5")];

function renderView({
  data = EVALUATED as unknown,
  results = HISTORY,
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
  render(
    <MockGadgetContextProvider
      runtime={{ sendMessage, logger }}
      launcher={{ dismiss, onExecute }}
    >
      <CalculatorView results={results} data={data} query="6*7" matchedPrefix="=" />
    </MockGadgetContextProvider>,
  );
  return { sendMessage, dismiss, onExecute, logError };
}

describe("CalculatorView", () => {
  it("copies the evaluated result on Enter and closes the launcher", async () => {
    const { sendMessage, dismiss, onExecute } = renderView();

    fireEvent.keyDown(document, { key: "Enter" });

    expect(sendMessage).toHaveBeenCalledWith("copy", EVALUATED);
    await waitFor(() => expect(dismiss).toHaveBeenCalledOnce());
    expect(onExecute).not.toHaveBeenCalled();
  });

  it("copies the selected history row on Enter", async () => {
    const { sendMessage, dismiss } = renderView();

    fireEvent.keyDown(document, { key: "ArrowDown" });
    fireEvent.keyDown(document, { key: "Enter" });

    expect(sendMessage).toHaveBeenCalledWith("copy", {
      expression: "1+1",
      result: "2",
      resultType: "number",
    });
    await waitFor(() => expect(dismiss).toHaveBeenCalledOnce());
  });

  it("copies a history row when it is clicked", async () => {
    const { sendMessage, dismiss } = renderView();

    fireEvent.click(screen.getByText("10/4"));

    expect(sendMessage).toHaveBeenCalledWith("copy", {
      expression: "10/4",
      result: "2.5",
      resultType: "number",
    });
    await waitFor(() => expect(dismiss).toHaveBeenCalledOnce());
  });

  it("does nothing on Enter without a result or a selected row", () => {
    const { sendMessage, dismiss } = renderView({ data: { expression: "" }, results: [] });

    fireEvent.keyDown(document, { key: "Enter" });

    expect(sendMessage).not.toHaveBeenCalled();
    expect(dismiss).not.toHaveBeenCalled();
  });

  it("keeps the launcher open and logs when copying fails", async () => {
    const sendMessage = vi.fn().mockRejectedValue(new Error("clipboard denied"));
    const { dismiss, logError } = renderView({ sendMessage });

    fireEvent.keyDown(document, { key: "Enter" });

    await waitFor(() => expect(logError).toHaveBeenCalled());
    expect(dismiss).not.toHaveBeenCalled();
  });
});
