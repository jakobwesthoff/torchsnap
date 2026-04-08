// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Barrel export for the plugin component infrastructure. Host
// code (Launcher, SettingsPanel) imports the provider; plugin
// components import the four hooks.

export {
  PluginContext,
  type PluginContextValue,
  type PluginInfo,
  type PluginRuntime,
  type PluginSendMessage,
  type LauncherActions,
} from "./PluginContext";
export { PluginContextProvider, type PluginContextProviderProps } from "./PluginContextProvider";
export { usePluginInfo } from "./usePluginInfo";
export { usePluginRuntime } from "./usePluginRuntime";
export { useLauncher } from "./useLauncher";
export { usePluginSetting } from "./usePluginSetting";
