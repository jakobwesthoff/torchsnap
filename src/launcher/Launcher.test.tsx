// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Launcher action errors (ADR 62)
//
// A failed action must leave visible feedback in the launcher
// footer, on every path a user or a gadget view can run one.
// The search, window lifecycle and mascot hooks are replaced so
// the test drives the results and views directly.
// =========================================================

import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useEffect } from "react";
import { KeyBindingProvider } from "../keybindings/KeyBindingProvider";
import { registerGadget, unregisterGadget } from "../gadgets/registry";
import { useLauncher } from "../contexts/useLauncher";
import { mockCommands } from "../test/tauri";
import type { GadgetViewRef, SourcedEntry } from "../types";
import type { useSearch } from "./hooks/useSearch";
import { Launcher } from "./Launcher";

type UseSearchResult = ReturnType<typeof useSearch>;

const state = vi.hoisted(() => ({
  search: {
    results: [],
    customGadgetView: null,
    inlineGadgetView: null,
    matchedPrefix: null,
    loading: false,
  } as UseSearchResult,
  dismiss: vi.fn(),
  resetState: (() => {}) as () => void,
}));

vi.mock("./hooks/useSearch", () => ({ useSearch: () => state.search }));
vi.mock("./hooks/useWindowLifecycle", () => ({
  useWindowLifecycle: ({ resetState }: { resetState: () => void }) => {
    state.resetState = resetState;
    return { hide: state.dismiss, dismiss: state.dismiss };
  },
}));
vi.mock("./hooks/useControlChannel", () => ({ useControlChannel: () => {} }));
vi.mock("./hooks/useLauncherMascotPlacement", () => ({ useLauncherMascotPlacement: () => null }));
vi.mock("../mascot", () => ({
  MascotDebugPanel: () => null,
  MascotInfoOverlay: () => null,
  useMascotVariant: () => ({ variant: "snappy", draw: null }),
}));
vi.mock("../hooks/useSetting", () => ({
  useSetting: (key: string) => [key === "mascotMode" ? "off" : true, vi.fn()],
}));

const EJECT: SourcedEntry = {
  id: "eject-disc",
  title: "Eject Disc",
  subtitle: null,
  icon: null,
  score: 1,
  titlePositions: [],
  subtitlePositions: [],
  source: "system-commands",
  actions: [{ slot: "primary", label: "Eject" }],
};

const EJECT_ERROR = "eject disc via drutil failed (exit status: 1): no drive";

function renderLauncher() {
  return render(
    <KeyBindingProvider>
      <Launcher />
    </KeyBindingProvider>,
  );
}

function input(): HTMLElement {
  return screen.getByPlaceholderText("Type to search");
}

function showView(view: GadgetViewRef, as: "custom" | "inline") {
  state.search = {
    results: [],
    customGadgetView: as === "custom" ? view : null,
    inlineGadgetView: as === "inline" ? view : null,
    matchedPrefix: null,
    loading: false,
  };
}

// A gadget view that runs one launcher action on mount and records
// how the promise it got back settled.
const outcome = vi.hoisted(() => ({ value: "" }));

function ActionView({ action }: { action: "execute" | "settings" | "showError" }) {
  const launcher = useLauncher();
  useEffect(() => {
    if (action === "showError") {
      launcher.showError("Copying the result failed: clipboard busy");
      return;
    }
    const run =
      action === "execute" ? launcher.onExecute("entry-1", "copy") : launcher.openSettings();
    run.then(
      () => (outcome.value = "resolved"),
      (e: unknown) => (outcome.value = `rejected: ${String(e)}`),
    );
    // Run once per mount.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return <div>view</div>;
}

describe("Launcher action errors", () => {
  beforeEach(() => {
    state.dismiss.mockClear();
    outcome.value = "";
    registerGadget("test-gadget", {
      label: "Test Gadget",
      views: {
        execute: () => <ActionView action="execute" />,
        settings: () => <ActionView action="settings" />,
        showError: () => <ActionView action="showError" />,
      },
      inlineViews: { execute: () => <ActionView action="execute" /> },
    });
  });

  afterEach(async () => {
    // The launcher's event listeners unregister asynchronously on
    // unmount. Let that finish while the mocked IPC still exists; the
    // shared setup clears it after this hook.
    cleanup();
    await new Promise((resolve) => setTimeout(resolve, 0));
    unregisterGadget("test-gadget");
    state.search = {
      results: [],
      customGadgetView: null,
      inlineGadgetView: null,
      matchedPrefix: null,
      loading: false,
    };
  });

  it("shows a failed entry action in the footer and keeps the launcher open", async () => {
    mockCommands({
      search_execute: () => Promise.reject(EJECT_ERROR),
    });
    state.search = { ...state.search, results: [EJECT] };
    renderLauncher();

    fireEvent.click(screen.getByText("Eject Disc"));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(`Eject Disc failed: ${EJECT_ERROR}`);
    expect(state.dismiss).not.toHaveBeenCalled();
  });

  it("shows no error and dismisses when the action succeeds", async () => {
    mockCommands({ search_execute: () => "Dismiss" });
    state.search = { ...state.search, results: [EJECT] };
    renderLauncher();

    fireEvent.click(screen.getByText("Eject Disc"));

    await waitFor(() => expect(state.dismiss).toHaveBeenCalledOnce());
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("clears the error on the next key press", async () => {
    mockCommands({ search_execute: () => Promise.reject(EJECT_ERROR) });
    state.search = { ...state.search, results: [EJECT] };
    renderLauncher();

    fireEvent.click(screen.getByText("Eject Disc"));
    await screen.findByRole("alert");

    fireEvent.keyDown(input(), { key: "x" });
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("clears the error when the launcher hides", async () => {
    mockCommands({ search_execute: () => Promise.reject(EJECT_ERROR) });
    state.search = { ...state.search, results: [EJECT] };
    renderLauncher();

    fireEvent.click(screen.getByText("Eject Disc"));
    await screen.findByRole("alert");

    act(() => state.resetState());
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("shows a custom view's failed onExecute under the gadget's name and rejects", async () => {
    mockCommands({ search_execute: () => Promise.reject("entry is gone") });
    showView({ gadgetId: "test-gadget", view: "execute" }, "custom");
    renderLauncher();

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Test Gadget failed: entry is gone");
    await waitFor(() => expect(outcome.value).toBe("rejected: entry is gone"));
  });

  it("shows an inline view's failed onExecute under the gadget's name", async () => {
    mockCommands({ search_execute: () => Promise.reject("entry is gone") });
    showView({ gadgetId: "test-gadget", view: "execute" }, "inline");
    renderLauncher();

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Test Gadget failed: entry is gone");
    await waitFor(() => expect(outcome.value).toBe("rejected: entry is gone"));
  });

  it("resolves a view's onExecute that succeeds", async () => {
    mockCommands({ search_execute: () => "Nothing" });
    showView({ gadgetId: "test-gadget", view: "execute" }, "custom");
    renderLauncher();

    await waitFor(() => expect(outcome.value).toBe("resolved"));
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("shows a failed openSettings and rejects", async () => {
    mockCommands({ gadget_open_settings: () => Promise.reject("window not created") });
    showView({ gadgetId: "test-gadget", view: "settings" }, "custom");
    renderLauncher();

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Opening Test Gadget settings failed: window not created");
    await waitFor(() => expect(outcome.value).toBe("rejected: window not created"));
    expect(state.dismiss).not.toHaveBeenCalled();
  });

  it("shows a view's own error as given", async () => {
    mockCommands({});
    showView({ gadgetId: "test-gadget", view: "showError" }, "custom");
    renderLauncher();

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Copying the result failed: clipboard busy");
  });
});
