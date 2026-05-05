// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Barrel export for the gadget component infrastructure. Host
// code (Launcher, SettingsPanel) imports the provider; gadget
// components import the four hooks.

export {
  GadgetContext,
  type GadgetContextValue,
  type GadgetInfo,
  type GadgetRuntime,
  type GadgetSendMessage,
  type LauncherActions,
} from "./GadgetContext";
export { GadgetContextProvider, type GadgetContextProviderProps } from "./GadgetContextProvider";
export { useGadgetInfo } from "./useGadgetInfo";
export { useGadgetRuntime } from "./useGadgetRuntime";
export { useLauncher } from "./useLauncher";
export { useGadgetSetting } from "./useGadgetSetting";
