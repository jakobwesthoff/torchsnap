// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Plugin SDK global injection.
 *
 * Exposes host-provided modules on `window.__torchsnap` so that
 * dynamically loaded plugin bundles can use React and the
 * plugin component context without bundling their own copies.
 *
 * Plugin bundles access these via shim files in `plugin-sdk/shims/`
 * that re-export from the global. At build time, Vite aliases
 * `"react"` and `"react/jsx-runtime"` to the shims; at runtime,
 * the shims read from `window.__torchsnap`.
 *
 * Future expansion (D5 in the migration plan): `components` and
 * `keybindings` will be added once the SDK shims for those land.
 */

import React from "react";
import * as jsxRuntime from "react/jsx-runtime";
import { usePluginInfo } from "../contexts/usePluginInfo";
import { usePluginRuntime } from "../contexts/usePluginRuntime";
import { useLauncher } from "../contexts/useLauncher";
import { usePluginSetting } from "../contexts/usePluginSetting";

// =========================================================
// Type declaration
// =========================================================

declare global {
  interface Window {
    __torchsnap?: {
      React: typeof React;
      jsxRuntime: typeof jsxRuntime;
      hooks: {
        usePluginInfo: typeof usePluginInfo;
        usePluginRuntime: typeof usePluginRuntime;
        useLauncher: typeof useLauncher;
        usePluginSetting: typeof usePluginSetting;
      };
    };
  }
}

// =========================================================
// Initialization
// =========================================================

/**
 * Set `window.__torchsnap` so plugin bundles can access
 * host-provided modules. Must be called before any plugin
 * frontend code loads.
 */
export function initPluginSdk(): void {
  window.__torchsnap = {
    React,
    jsxRuntime,
    hooks: {
      usePluginInfo,
      usePluginRuntime,
      useLauncher,
      usePluginSetting,
    },
  };
}
