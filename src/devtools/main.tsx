// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { startWindow } from "../lib/startWindow";
import { initStore } from "../settingsStore";
import { DevToolsPanel } from "./DevToolsPanel";
import "../index.css";

void startWindow({
  window: "devtools",
  prepare: initStore,
  app: () => <DevToolsPanel />,
});
