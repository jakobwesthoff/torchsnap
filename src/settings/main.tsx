// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { command } from "../lib/command";
import { initStore } from "../settingsStore";
import { preloadSettingsComponents } from "../lib/gadgetComponent";
import { initGadgetSdk } from "../lib/sdk";
import { startWindow } from "../lib/startWindow";
import { registerAllWasmGadgets } from "../gadgets/wasmPluginLoader";
import { SettingsPanel } from "./SettingsPanel";
import "../index.css";

// The settings store must be loaded before React mounts so that
// `useSetting` can read values synchronously on the first render.
// See settingsStore.ts for the full explanation.
void startWindow({
  window: "settings",
  prepare: async () => {
    await initStore();
    initGadgetSdk();

    const wasmGadgets = await command("wasm_gadgets");
    registerAllWasmGadgets(wasmGadgets, "settings");

    preloadSettingsComponents();
  },
  app: () => <SettingsPanel />,
});
