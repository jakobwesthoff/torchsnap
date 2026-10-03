// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { act, fireEvent, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { actionErrorMessage, useActionError } from "./useActionError";

describe("actionErrorMessage", () => {
  it("leads with what failed, then the backend's message", () => {
    expect(actionErrorMessage("Eject Disc", "eject disc via drutil failed")).toBe(
      "Eject Disc failed: eject disc via drutil failed",
    );
  });

  it("uses an Error's message", () => {
    expect(actionErrorMessage("Clipboard", new Error("no entry"))).toBe(
      "Clipboard failed: no entry",
    );
  });
});

describe("useActionError", () => {
  function render(resetKey = "a") {
    return renderHook(({ key }) => useActionError(key), { initialProps: { key: resetKey } });
  }

  it("shows the error of a failed action and rejects with it", async () => {
    const { result } = render();

    await act(async () => {
      await expect(
        result.current.runAction("Eject Disc", () => Promise.reject("no drive")),
      ).rejects.toBe("no drive");
    });

    expect(result.current.error).toBe("Eject Disc failed: no drive");
  });

  it("resolves with the action's value and shows nothing on success", async () => {
    const { result } = render();

    let value: number | undefined;
    await act(async () => {
      value = await result.current.runAction("Copy", () => Promise.resolve(42));
    });

    expect(value).toBe(42);
    expect(result.current.error).toBeNull();
  });

  it("clears the previous error when the next action starts", async () => {
    const { result } = render();
    act(() => result.current.showError("copy failed"));

    let finish: () => void = () => {};
    let running: Promise<void> = Promise.resolve();
    act(() => {
      running = result.current.runAction(
        "Paste",
        () => new Promise<void>((resolve) => (finish = resolve)),
      );
    });
    expect(result.current.error).toBeNull();

    await act(async () => {
      finish();
      await running;
    });
    expect(result.current.error).toBeNull();
  });

  it("shows a view's own message as given", () => {
    const { result } = render();
    act(() => result.current.showError("Copying the result failed: clipboard busy"));
    expect(result.current.error).toBe("Copying the result failed: clipboard busy");
  });

  it("clears when the reset key changes", () => {
    const { result, rerender } = render("query=a");
    act(() => result.current.showError("copy failed"));

    rerender({ key: "query=a" });
    expect(result.current.error).toBe("copy failed");

    rerender({ key: "query=ab" });
    expect(result.current.error).toBeNull();
  });

  it("clears on a key press but not on a lone modifier", () => {
    const { result } = render();
    act(() => result.current.showError("copy failed"));

    act(() => {
      fireEvent.keyDown(window, { key: "Meta" });
      fireEvent.keyDown(window, { key: "Shift" });
    });
    expect(result.current.error).toBe("copy failed");

    act(() => {
      fireEvent.keyDown(window, { key: "ArrowDown" });
    });
    expect(result.current.error).toBeNull();
  });

  it("clears on request", () => {
    const { result } = render();
    act(() => result.current.showError("copy failed"));
    act(() => result.current.clearError());
    expect(result.current.error).toBeNull();
  });
});
