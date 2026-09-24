// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { ThemeProvider } from "../contexts/ThemeProvider";
import { UpdateApp } from "./UpdateApp";
import "../index.css";

// The update window reads no settings, so unlike Settings it does not
// load the settings store; its capability does not grant it either.
async function main() {
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <ThemeProvider>
        <UpdateApp />
      </ThemeProvider>
    </StrictMode>,
  );

  // Signal the backend that the UI has rendered so it can show the
  // window without flashing an empty frame.
  await getCurrentWebviewWindow().emit("react-ready");
}

main();
