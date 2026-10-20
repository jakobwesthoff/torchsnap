// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { emitTauriEvent, mockCommands } from "../test/tauri";
import { GadgetEnableFailureNote } from "./GadgetEnableFailureNote";
import { GADGET_ENABLE_FAILURES_CHANGED } from "./useGadgetEnableFailure";

/** A backend whose failures the test changes, and whose retries it
 *  answers, or holds until it calls the returned `finish`. */
function backend(initial: Record<string, string>) {
  const state = {
    failures: initial,
    retries: [] as unknown[],
    pending: null as null | { finish: () => void; fail: () => void },
    holdRetries: false,
    rejectRetries: false,
  };
  mockCommands({
    gadget_enable_failures: () => state.failures,
    gadget_retry_enable: (params) => {
      state.retries.push(params);
      if (state.rejectRetries) {
        return Promise.reject(new Error("the gadget is turned off"));
      }
      if (!state.holdRetries) {
        return undefined;
      }
      return new Promise<void>((resolve, reject) => {
        state.pending = { finish: resolve, fail: () => reject(new Error("failed")) };
      });
    },
  });
  return state;
}

async function shownNote() {
  render(<GadgetEnableFailureNote gadgetId="awake" name="Awake" />);
  return screen.findByRole("alert");
}

function retryButton() {
  return screen.getByRole("button", { name: /^Retry/ });
}

describe("GadgetEnableFailureNote", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("names the gadget in its title and shows the reason on its own", async () => {
    backend({ awake: "Amphetamine is not installed" });

    const note = await shownNote();

    expect(within(note).getByText("Awake failed to start")).toBeInTheDocument();
    expect(within(note).getByText("Amphetamine is not installed").textContent).toBe(
      "Amphetamine is not installed",
    );
    expect(note).not.toHaveTextContent("(");
    expect(within(note).getByRole("button", { name: "Retry" })).toBeEnabled();
  });

  it("shows nothing while the gadget runs", async () => {
    let asked = false;
    mockCommands({
      gadget_enable_failures: () => {
        asked = true;
        return { calculator: "broken" };
      },
    });

    const { container } = render(<GadgetEnableFailureNote gadgetId="awake" name="Awake" />);

    await waitFor(() => expect(asked).toBe(true));
    expect(container).toBeEmptyDOMElement();
  });

  it("asks the backend to start the gadget again", async () => {
    const state = backend({ awake: "Amphetamine is not installed" });
    await shownNote();

    fireEvent.click(retryButton());

    await waitFor(() => expect(state.retries).toEqual([{ gadgetId: "awake" }]));
  });

  it("shows Retrying… for at least 600 ms when the retry fails at once", async () => {
    backend({ awake: "Amphetamine is not installed" });
    await shownNote();
    vi.useFakeTimers();

    fireEvent.click(retryButton());
    await act(() => vi.advanceTimersByTimeAsync(599));

    expect(retryButton()).toHaveTextContent("Retrying…");
    expect(retryButton()).toBeDisabled();

    await act(() => vi.advanceTimersByTimeAsync(1));

    expect(retryButton()).toHaveTextContent(/^Retry$/);
    expect(retryButton()).toBeEnabled();
  });

  it("keeps the block and its reason while the retry runs", async () => {
    const state = backend({ awake: "Amphetamine is not installed" });
    state.holdRetries = true;
    const note = await shownNote();
    vi.useFakeTimers();

    fireEvent.click(retryButton());
    // The backend clears the failure when the retry starts and records
    // it again when the gadget fails once more.
    state.failures = {};
    await act(() => emitTauriEvent(GADGET_ENABLE_FAILURES_CHANGED));
    await act(() => vi.advanceTimersByTimeAsync(0));

    expect(note).toBeInTheDocument();
    expect(within(note).getByText("Amphetamine is not installed")).toBeInTheDocument();
    expect(retryButton()).toHaveTextContent("Retrying…");

    state.failures = { awake: "Amphetamine is still not installed" };
    await act(() => emitTauriEvent(GADGET_ENABLE_FAILURES_CHANGED));
    await act(() => vi.advanceTimersByTimeAsync(600));
    expect(retryButton()).toHaveTextContent("Retrying…");

    await act(async () => state.pending?.finish());
    await act(() => vi.advanceTimersByTimeAsync(0));

    expect(screen.getAllByRole("alert")).toHaveLength(1);
    expect(screen.getByText("Amphetamine is still not installed")).toBeInTheDocument();
    expect(retryButton()).toHaveTextContent(/^Retry$/);
  });

  it("goes away when the retry starts the gadget", async () => {
    const state = backend({ awake: "Amphetamine is not installed" });
    state.holdRetries = true;
    await shownNote();
    vi.useFakeTimers();

    fireEvent.click(retryButton());
    state.failures = {};
    await act(() => emitTauriEvent(GADGET_ENABLE_FAILURES_CHANGED));
    await act(async () => state.pending?.finish());
    await act(() => vi.advanceTimersByTimeAsync(600));

    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("offers Retry again when the retry is refused", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const state = backend({ awake: "Amphetamine is not installed" });
    state.rejectRetries = true;
    await shownNote();
    vi.useFakeTimers();

    fireEvent.click(retryButton());
    await act(() => vi.advanceTimersByTimeAsync(600));

    expect(retryButton()).toHaveTextContent(/^Retry$/);
    expect(retryButton()).toBeEnabled();
    expect(warn).toHaveBeenCalledWith('command("gadget_retry_enable") failed:', expect.anything());
  });

  it("disables Retry when the gadget cannot be started from here", async () => {
    backend({ awake: "Amphetamine is not installed" });

    render(<GadgetEnableFailureNote gadgetId="awake" name="Awake" retryDisabled />);

    expect(await screen.findByRole("button", { name: "Retry" })).toBeDisabled();
  });
});
