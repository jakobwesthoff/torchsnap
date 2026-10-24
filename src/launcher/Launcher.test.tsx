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
import { emitTauriEvent, mockCommands } from "../test/tauri";
import type { GadgetViewProps } from "../gadgets/types";
import type { GadgetViewRef, LauncherSnapshot, SourcedEntry } from "../types";
import type { useSearch } from "./hooks/useSearch";
import { Launcher } from "./Launcher";
import { CARD_SHADOW, CARD_TOP_OFFSET } from "./layout";

type UseSearchResult = ReturnType<typeof useSearch>;

const state = vi.hoisted(() => ({
  search: {
    results: [],
    customGadgetView: null,
    inlineGadgetView: null,
    matchedPrefix: null,
    resultsQuery: "",
    loading: false,
  } as UseSearchResult,
  dismiss: vi.fn(),
  resetState: (() => {}) as () => void,
  getSnapshot: null as (() => LauncherSnapshot) | null,
  setQuery: null as ((text: string) => void) | null,
}));

vi.mock("./hooks/useSearch", () => ({ useSearch: () => state.search }));
vi.mock("./hooks/useWindowLifecycle", () => ({
  useWindowLifecycle: ({ resetState }: { resetState: () => void }) => {
    state.resetState = resetState;
    return { hide: state.dismiss, dismiss: state.dismiss };
  },
}));
vi.mock("./hooks/useControlChannel", () => ({
  useControlChannel: ({
    getSnapshot,
    setQuery,
  }: {
    getSnapshot: () => LauncherSnapshot;
    setQuery: (text: string) => void;
  }) => {
    state.getSnapshot = getSnapshot;
    state.setQuery = setQuery;
  },
}));
vi.mock("./visibility", () => ({ isContentVisible: () => true }));
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
    resultsQuery: "",
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
      resultsQuery: "",
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

// =========================================================
// The control API's snapshot (ADR 69)
// =========================================================

describe("Launcher snapshot", () => {
  afterEach(async () => {
    cleanup();
    await new Promise((resolve) => setTimeout(resolve, 0));
    state.getSnapshot = null;
    state.search = {
      results: [],
      customGadgetView: null,
      inlineGadgetView: null,
      matchedPrefix: null,
      resultsQuery: "",
      loading: false,
    };
  });

  it("reports the typed query, the results and the selection", () => {
    mockCommands({});
    state.search = { ...state.search, results: [EJECT], loading: true };
    renderLauncher();

    fireEvent.change(input(), { target: { value: "eject" } });

    expect(state.getSnapshot?.()).toEqual({
      query: "eject",
      searching: true,
      results: [
        { source: "system-commands", id: "eject-disc", title: "Eject Disc", subtitle: null },
      ],
      selectedIndex: 0,
      inlineView: null,
      inlineViewSelected: false,
      gadgetView: null,
      contentVisible: true,
      inputFocused: false,
    });
  });

  it("reports the input as focused when it is the active element", () => {
    mockCommands({});
    renderLauncher();

    input().focus();

    expect(state.getSnapshot?.().inputFocused).toBe(true);
  });
});

// =========================================================
// A view's query while the search for new text runs
//
// The view on screen and its matched prefix belong to the last search
// that answered. New text that no longer starts with that prefix must
// reach the input as it was set, also when the view writes its
// `matchedPrefix + query` back into the input, as the calculator's
// history view does.
// =========================================================

describe("Launcher query across a prefix change", () => {
  // Writes `matchedPrefix + query` into the input whenever either
  // changes, and shows the query it got.
  function EchoView({ query, matchedPrefix }: GadgetViewProps) {
    const { setDisplayQuery } = useLauncher();
    useEffect(() => {
      setDisplayQuery(`${matchedPrefix}${query}`);
    }, [matchedPrefix, query, setDisplayQuery]);
    return <div data-testid="echo-query">{query}</div>;
  }

  const ECHO_VIEW: GadgetViewRef = { gadgetId: "test-gadget", view: "echo" };

  const HELLO: SourcedEntry = { ...EJECT, id: "hello", title: "Hello b2", source: "hello-world" };

  /** What useSearch returns once the search for `=2+3` answered. */
  const CALCULATOR_ANSWERED: UseSearchResult = {
    results: [],
    customGadgetView: ECHO_VIEW,
    inlineGadgetView: null,
    matchedPrefix: "=",
    resultsQuery: "=2+3",
    loading: false,
  };

  beforeEach(() => {
    mockCommands({});
    registerGadget("test-gadget", { label: "Test Gadget", views: { echo: EchoView } });
  });

  afterEach(async () => {
    cleanup();
    await new Promise((resolve) => setTimeout(resolve, 0));
    unregisterGadget("test-gadget");
    state.setQuery = null;
    state.search = {
      results: [],
      customGadgetView: null,
      inlineGadgetView: null,
      matchedPrefix: null,
      resultsQuery: "",
      loading: false,
    };
  });

  function rerenderLauncher(rerender: (ui: React.ReactElement) => void) {
    rerender(
      <KeyBindingProvider>
        <Launcher />
      </KeyBindingProvider>,
    );
  }

  /** The launcher showing the view for `=2+3`. `state.search` stays at
   *  that answer until a test replaces it, which stands for the time
   *  before the search for any later text answers. */
  function renderCalculatorQuery() {
    state.search = CALCULATOR_ANSWERED;
    const launcher = renderLauncher();
    fireEvent.change(input(), { target: { value: "=2+3" } });
    expect(input()).toHaveValue("=2+3");
    return launcher;
  }

  function viewQuery(): string | null {
    return screen.getByTestId("echo-query").textContent;
  }

  it("shows a control query with another prefix as it was set", () => {
    renderCalculatorQuery();

    act(() => state.setQuery!("!b2"));

    expect(input()).toHaveValue("!b2");
  });

  it("shows text typed over the whole input as typed", () => {
    renderCalculatorQuery();

    fireEvent.change(input(), { target: { value: "a" } });

    expect(input()).toHaveValue("a");
  });

  it("shows the text left after deleting the prefix", () => {
    renderCalculatorQuery();

    fireEvent.change(input(), { target: { value: "2+3" } });

    expect(input()).toHaveValue("2+3");
  });

  it("hands the view the query of its own search until the new search answers", () => {
    renderCalculatorQuery();

    act(() => state.setQuery!("!b2"));

    expect(viewQuery()).toBe("2+3");
  });

  it("hands the view text typed after its prefix before the search answers", () => {
    renderCalculatorQuery();

    fireEvent.change(input(), { target: { value: "=2+34" } });

    expect(viewQuery()).toBe("2+34");
    expect(input()).toHaveValue("=2+34");
  });

  it("keeps the new text in the input once the new search answers", () => {
    const { rerender } = renderCalculatorQuery();
    act(() => state.setQuery!("!b2"));

    state.search = {
      results: [HELLO],
      customGadgetView: null,
      inlineGadgetView: null,
      matchedPrefix: "!",
      resultsQuery: "!b2",
      loading: false,
    };
    rerenderLauncher(rerender);

    expect(screen.queryByTestId("echo-query")).toBeNull();
    expect(input()).toHaveValue("!b2");
  });

  it("hands an execute-triggered view the query of its own search, then the new one", async () => {
    const { rerender } = renderLauncher();
    await act(() => emitTauriEvent("activate-gadget-custom-ui", ECHO_VIEW));
    await waitFor(() => expect(viewQuery()).toBe(""));

    // The view opened by execute stays while the search for `=2`
    // routes to a prefix gadget.
    state.search = {
      results: [],
      customGadgetView: null,
      inlineGadgetView: null,
      matchedPrefix: "=",
      resultsQuery: "=2",
      loading: false,
    };
    fireEvent.change(input(), { target: { value: "=2" } });
    expect(viewQuery()).toBe("2");

    fireEvent.change(input(), { target: { value: "x" } });
    expect(input()).toHaveValue("x");
    expect(viewQuery()).toBe("2");

    state.search = { ...state.search, matchedPrefix: null, resultsQuery: "x" };
    rerenderLauncher(rerender);
    expect(viewQuery()).toBe("x");
    expect(input()).toHaveValue("x");
  });
});

// =========================================================
// The card's place and shadow in the window
//
// The window size the backend gets is computed from the constants in
// layout.ts, so the page must place the card and draw its shadow from
// the same constants.
// =========================================================

describe("Launcher card layout", () => {
  afterEach(async () => {
    cleanup();
    await new Promise((resolve) => setTimeout(resolve, 0));
  });

  it("puts the card's top the card offset below the window's top", () => {
    mockCommands({});
    const { container } = renderLauncher();

    const root = container.firstElementChild as HTMLElement;
    expect(root.style.paddingTop).toBe(`${CARD_TOP_OFFSET}px`);
  });

  it("draws the card's shadow from its layers", () => {
    mockCommands({});
    renderLauncher();

    const card = input().closest<HTMLElement>('[class~="w-[680px]"]');
    expect(card?.style.boxShadow).toBe(CARD_SHADOW.join(", "));
  });
});
