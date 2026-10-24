// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Launcher startup: the layout handshake
//
// The launcher entry measures the card at its largest and sends the
// window layout to the backend, which sizes and places the window
// from it on every show. The startup modules are replaced, and the
// launcher is a stand-in whose measure render reports a card of a
// known size.
// =========================================================

import { waitFor } from "@testing-library/react";
import { useEffect } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CommandMap } from "../lib/command";
import { mockCommands } from "../test/tauri";
import { launcherWindowLayout } from "./layout";

// Width and height differ, so swapped arguments change the layout.
const CARD = { width: 700, height: 500 };

vi.mock("../settingsStore", () => ({ initStore: () => Promise.resolve() }));
vi.mock("./visibility", () => ({ setupLauncherVisibilityChoreography: () => Promise.resolve() }));
vi.mock("../lib/sdk", () => ({ initGadgetSdk: () => {} }));
vi.mock("../gadgets/wasmPluginLoader", () => ({ registerAllWasmGadgets: () => {} }));
vi.mock("../lib/gadgetComponent", () => ({ preloadLauncherComponents: () => {} }));
vi.mock("../contexts/ThemeProvider", () => ({
  ThemeProvider: ({ children }: { children: React.ReactNode }) => children,
}));
vi.mock("../keybindings", () => ({
  KeyBindingProvider: ({ children }: { children: React.ReactNode }) => children,
}));
vi.mock("./Launcher", () => ({
  Launcher: ({
    measureDummy,
    onMeasure,
  }: {
    measureDummy?: boolean;
    onMeasure?: (cardWidth: number, cardHeight: number) => void;
  }) => {
    useEffect(() => {
      if (measureDummy) onMeasure?.(CARD.width, CARD.height);
    }, [measureDummy, onMeasure]);
    return null;
  },
}));

describe("launcher startup", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("sends the window layout for the measured card", async () => {
    const layouts: CommandMap["launcher_set_layout"]["params"][] = [];
    mockCommands({
      wasm_gadgets: () => [],
      launcher_set_layout: (params) => {
        layouts.push(params);
      },
    });
    document.body.innerHTML = '<div id="root"></div>';

    await import("./main");

    // StrictMode runs the measure effect twice in development, so the
    // backend may get the same layout twice; every one must match.
    await waitFor(() => expect(layouts.length).toBeGreaterThan(0));
    for (const layout of layouts) {
      expect(layout).toEqual(launcherWindowLayout(CARD.width, CARD.height));
    }
  });
});
