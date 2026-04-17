// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { command } from "../lib/command";
import { ThemeProvider } from "../contexts/ThemeProvider";
import { KeyBindingProvider } from "../keybindings";
import { initStore } from "../settingsStore";
import { preloadLauncherComponents } from "../lib/pluginComponent";
import { initPluginSdk } from "../lib/sdk";
import { registerAllWasmPlugins } from "../plugins/wasmPluginLoader";
import { Launcher } from "./Launcher";
import { SHADOW_PADDING, MASCOT_HEADROOM, CARD_TOP_OFFSET } from "./layout";
import "../index.css";

// The settings store must be loaded before React mounts so that
// `useSetting` can read values synchronously on the first render.
// The backend guarantees all default values are present in the store
// before any webview is created, so no fallback defaults are needed
// on the frontend.
async function main() {
  await initStore();

  // Initialize the plugin SDK global before any plugin code loads.
  initPluginSdk();

  // Register WASM plugin frontend components before preloading.
  // This ensures dynamic import() factories for WASM plugins are
  // set up and included in the preload batch.
  const wasmPlugins = await command("wasm_plugins");
  registerAllWasmPlugins(wasmPlugins, "launcher");

  preloadLauncherComponents();

  const root = createRoot(document.getElementById("root")!);

  // Phase 1: Render the launcher in measurement mode. The card
  // renders at its maximum possible size (search bar + max-height
  // content area + footer). The onMeasure callback fires once the
  // ResizeObserver reports the card's dimensions, at which point
  // we compute the window size and notify the backend.
  //
  // Phase 2: Re-render without measurement props. The launcher
  // switches to normal mode with no ResizeObserver overhead.

  await new Promise<void>((resolve) => {
    root.render(
      <StrictMode>
        <ThemeProvider>
          <KeyBindingProvider>
            <Launcher
              measureDummy
              onMeasure={async (cardWidth, cardHeight) => {
                await command("launcher_set_layout", {
                  windowWidth: cardWidth + 2 * SHADOW_PADDING,
                  windowHeight: SHADOW_PADDING + MASCOT_HEADROOM + cardHeight + SHADOW_PADDING,
                  cardTopOffset: CARD_TOP_OFFSET,
                });
                resolve();
              }}
            />
          </KeyBindingProvider>
        </ThemeProvider>
      </StrictMode>,
    );
  });

  root.render(
    <StrictMode>
      <ThemeProvider>
        <KeyBindingProvider>
          <Launcher />
        </KeyBindingProvider>
      </ThemeProvider>
    </StrictMode>,
  );
}

main();
