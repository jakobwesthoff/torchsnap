// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { WindowErrorBoundary } from "../components/WindowErrorBoundary";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { ThemeProvider } from "../contexts/ThemeProvider";
import { initStore } from "../settingsStore";
import { WelcomeApp } from "./WelcomeApp";
import "../index.css";

// The shortcut step reads and writes `globalShortcut`, so the settings
// store is loaded before React mounts, as in Settings.
async function main() {
  await initStore();

  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <WindowErrorBoundary window="welcome">
        <ThemeProvider>
          <WelcomeApp />
        </ThemeProvider>
      </WindowErrorBoundary>
    </StrictMode>,
  );

  // Signal the backend that the UI has rendered so it can show the
  // window without flashing an empty frame.
  await getCurrentWebviewWindow().emit("react-ready");
}

main();
