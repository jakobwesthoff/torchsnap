// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { ThemeProvider } from "../contexts/ThemeProvider";
import { KeyBindingProvider } from "../keybindings";
import { initStore } from "../settingsStore";
import { Launcher } from "./Launcher";
import "../index.css";

// The settings store must be loaded before React mounts so that
// `useSetting` can read values synchronously on the first render.
// The backend guarantees all default values are present in the store
// before any webview is created, so no fallback defaults are needed
// on the frontend.
async function main() {
  await initStore();

  createRoot(document.getElementById("root")!).render(
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
