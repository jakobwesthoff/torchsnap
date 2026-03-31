// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { ThemeProvider } from "../contexts/ThemeProvider";
import { initStore } from "../settingsStore";
import { preloadSettingsComponents } from "../lib/pluginComponent";
import { SettingsPanel } from "./SettingsPanel";
import "../index.css";

// The settings store must be loaded before React mounts so that
// `useSetting` can read values synchronously on the first render.
// See settingsStore.ts for the full explanation.
async function main() {
  await initStore();
  preloadSettingsComponents();

  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <ThemeProvider>
        <SettingsPanel />
      </ThemeProvider>
    </StrictMode>,
  );

  // Signal the Rust backend that the settings UI has rendered so it
  // can make the window visible without flashing an empty frame.
  // NOTE: This fires right after `render()` returns, which schedules
  // the React tree but may not have painted yet. In practice the
  // round-trip through Tauri's event system and `run_on_main_thread`
  // adds enough latency that the first frame is composited before
  // the window becomes visible — but this is not guaranteed.
  await getCurrentWebviewWindow().emit("react-ready");
}

main();
