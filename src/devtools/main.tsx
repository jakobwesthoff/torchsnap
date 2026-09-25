// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { WindowErrorBoundary } from "../components/WindowErrorBoundary";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { ThemeProvider } from "../contexts/ThemeProvider";
import { initStore } from "../settingsStore";
import { DevToolsPanel } from "./DevToolsPanel";
import "../index.css";

async function main() {
  await initStore();

  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <WindowErrorBoundary window="devtools">
        <ThemeProvider>
          <DevToolsPanel />
        </ThemeProvider>
      </WindowErrorBoundary>
    </StrictMode>,
  );

  // Signal the Rust backend that the UI has rendered so it
  // can make the window visible without flashing an empty frame.
  await getCurrentWebviewWindow().emit("react-ready");
}

main();
