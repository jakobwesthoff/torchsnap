// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { startWindow } from "../lib/startWindow";
import { initStore } from "../settingsStore";
import { WelcomeApp } from "./WelcomeApp";
import "../index.css";

// The shortcut step reads and writes `globalShortcut`, so the settings
// store is loaded before React mounts, as in Settings.
void startWindow({
  window: "welcome",
  prepare: initStore,
  app: () => <WelcomeApp />,
});
