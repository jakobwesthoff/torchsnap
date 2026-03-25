// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { ThemeProvider } from "../contexts/ThemeProvider";
import { KeyBindingProvider } from "../keybindings";
import { Launcher } from "./Launcher";
import "../index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider>
      <KeyBindingProvider>
        <Launcher />
      </KeyBindingProvider>
    </ThemeProvider>
  </StrictMode>,
);
