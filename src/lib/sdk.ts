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
import { highlightText } from "./highlightText";

// =========================================================
// Type declaration
//
// Declares the full shape of `window.__torchsnap` for the host
// compilation. TypeScript module augmentation is scope-limited
// to files that (transitively) import the augmenting module —
// so `sdk.ts` must declare the complete `TorchsnapGlobal` shape
// here using `typeof` the actual implementations, because the
// shim augmentations are only visible in files that import the
// shims. The shim type simplifications (e.g. `string[]` instead
// of `ModifierKey[]`) are intentional for plugin ergonomics and
// do not affect the host assignment below.
// =========================================================

declare global {
  // `React` and `jsxRuntime` are host-only slices that the plugin
  // shims don't expose. `keybindings` and `components` are declared
  // here because the shim augmentations for those slices are not in
  // `sdk.ts`'s transitive import graph (they come from
  // `plugin-sdk/src/shims/keybindings.ts` and `components.ts`, which
  // nothing in `src/` imports except `PluginContext.tsx`'s type-sync
  // check — and that only imports `hooks.ts`). `hooks` is intentionally
  // omitted: `hooks.ts` IS transitively reachable and its augmentation
  // is already in scope; re-declaring it here would cause a TS2717
  // conflict.
  interface TorchsnapGlobal {
    React: typeof React;
    jsxRuntime: typeof jsxRuntime;
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
    utils: {
      highlightText: typeof highlightText;
    };
  }
  interface Window {
    __torchsnap?: TorchsnapGlobal;
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
    utils: {
      highlightText,
    },
  };
}
