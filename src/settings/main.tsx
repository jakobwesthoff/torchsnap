// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { ThemeProvider } from "../contexts/ThemeProvider";
import { initStore } from "../settingsStore";
import { SettingsPanel } from "./SettingsPanel";
import "../index.css";

// The settings store must be loaded before React mounts so that
// `useSetting` can read values synchronously on the first render.
// See settingsStore.ts for the full explanation.
async function main() {
  await initStore();

  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <ThemeProvider>
        <SettingsPanel />
      </ThemeProvider>
    </StrictMode>,
  );
}

main();
