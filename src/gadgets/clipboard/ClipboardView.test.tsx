// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { GadgetContextProvider } from "../../contexts/GadgetContextProvider";
import type { GadgetRuntime, LauncherActions } from "../../contexts/GadgetContext";
import { KeyBindingProvider } from "../../keybindings/KeyBindingProvider";
import { platform } from "../../keybindings/platform";
import ClipboardView from "./ClipboardView";
import type { ClipboardHistoryEntry, ClipboardListEntry } from "./types";

const ENTRY: ClipboardListEntry = {
  id: "entry-1",
  capturedAt: "2026-10-03T12:00:00Z",
  displayText: "hello world",
  primaryFormat: "text",
};

const DETAIL: ClipboardHistoryEntry = {
  ...ENTRY,
  formats: { "public.utf8-plain-text": { type: "string", data: "hello world" } },
};

/** `sendMessage` that serves the history and answers `paste` and
 *  `delete` with the given outcome. */
function backend(action: { fails: boolean }) {
  return vi.fn(async (method: string, _payload: unknown, onMessage?: (msg: unknown) => void) => {
    switch (method) {
      case "search":
        onMessage?.([ENTRY]);
        return null;
      case "load_full_entry":
        return DETAIL;
      case "paste":
      case "delete":
        if (action.fails) throw "no entry with id entry-1";
        return null;
      default:
        throw new Error(`unexpected message ${method}`);
    }
  });
}

function renderView(sendMessage: ReturnType<typeof backend>) {
  const dismiss = vi.fn();
  const showError = vi.fn();
  const runtime = {
    sendMessage,
    logger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn(), trace: vi.fn() },
  } as unknown as GadgetRuntime;
  const launcher: LauncherActions = {
    goBack: vi.fn(),
    dismiss,
    openSettings: vi.fn(async () => {}),
    onExecute: vi.fn(async () => {}),
    showError,
    onFooterChange: vi.fn(),
    setDisplayQuery: vi.fn(),
    mouseActiveRef: { current: false },
  };
  render(
    <KeyBindingProvider>
      <GadgetContextProvider
        info={{ id: "clipboard-manager", enabled: true }}
        runtime={runtime}
        launcher={launcher}
      >
        <ClipboardView query="" matchedPrefix="" results={[]} />
      </GadgetContextProvider>
    </KeyBindingProvider>,
  );
  return { dismiss, showError };
}

// `Meta` in a keybinding is Cmd on macOS and Ctrl elsewhere.
const META = platform === "macos" ? { metaKey: true } : { ctrlKey: true };

describe("ClipboardView actions", () => {
  it("copies the selected entry and closes the launcher", async () => {
    const sendMessage = backend({ fails: false });
    const { dismiss, showError } = renderView(sendMessage);
    await screen.findAllByText("hello world");

    fireEvent.keyDown(document, { key: "Enter" });

    await waitFor(() => expect(dismiss).toHaveBeenCalledOnce());
    expect(sendMessage).toHaveBeenCalledWith("paste", { id: "entry-1" });
    expect(showError).not.toHaveBeenCalled();
  });

  it("keeps the launcher open and shows the error when copying fails", async () => {
    const { dismiss, showError } = renderView(backend({ fails: true }));
    await screen.findAllByText("hello world");

    fireEvent.keyDown(document, { key: "Enter" });

    await waitFor(() =>
      expect(showError).toHaveBeenCalledWith("Copying the entry failed: no entry with id entry-1"),
    );
    expect(dismiss).not.toHaveBeenCalled();
  });

  it("removes the selected entry", async () => {
    const sendMessage = backend({ fails: false });
    const { showError } = renderView(sendMessage);
    await screen.findAllByText("hello world");

    fireEvent.keyDown(document, { key: "Backspace", ...META });

    await waitFor(() => expect(sendMessage).toHaveBeenCalledWith("delete", { id: "entry-1" }));
    expect(showError).not.toHaveBeenCalled();
  });

  it("shows the error when removing fails", async () => {
    const { showError } = renderView(backend({ fails: true }));
    await screen.findAllByText("hello world");

    fireEvent.keyDown(document, { key: "Backspace", ...META });

    await waitFor(() =>
      expect(showError).toHaveBeenCalledWith("Removing the entry failed: no entry with id entry-1"),
    );
  });
});
