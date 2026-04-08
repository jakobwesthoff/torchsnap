// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Plugin SDK global injection.
 *
 * Exposes host-provided modules on `window.__torchsnap` so that
 * dynamically loaded plugin bundles can use React, the plugin
 * component context, host components, and host hooks without
 * bundling their own copies.
 *
 * Plugin bundles access these via shim files in `plugin-sdk/src/shims/`
 * that re-export from the global. At build time, Vite aliases
 * `"react"` and `"react/jsx-runtime"` to the React shims; the other
 * subpaths are real package paths exported from `plugin-sdk/package.json`
 * and resolved via standard module resolution.
 */

import React from "react";
import * as jsxRuntime from "react/jsx-runtime";
import { usePluginInfo } from "../contexts/usePluginInfo";
import { usePluginRuntime } from "../contexts/usePluginRuntime";
import { useLauncher } from "../contexts/useLauncher";
import { usePluginSetting } from "../contexts/usePluginSetting";
import { useWindowedList } from "../launcher/hooks/useWindowedList";
import { useKeyBindings } from "../keybindings/useKeyBindings";
import { LAYER } from "../keybindings/matching";
import { Switch } from "../components/Switch";
import { Slider } from "../components/Slider";
import { Section } from "../settings/Section";
import { Entry } from "../settings/Entry";

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
        useWindowedList: typeof useWindowedList;
      };
      keybindings: {
        useKeyBindings: typeof useKeyBindings;
        LAYER: typeof LAYER;
      };
      components: {
        Switch: typeof Switch;
        Slider: typeof Slider;
        Section: typeof Section;
        Entry: typeof Entry;
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
      useWindowedList,
    },
    keybindings: {
      useKeyBindings,
      LAYER,
    },
    components: {
      Switch,
      Slider,
      Section,
      Entry,
    },
  };
}
