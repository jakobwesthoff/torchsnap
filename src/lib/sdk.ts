// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Plugin SDK global injection.
 *
 * Exposes host-provided modules on `window.__torchsnap` so that
 * dynamically loaded plugin bundles can use React without bundling
 * their own copy.
 *
 * Plugin authors access the global at runtime:
 *
 *   const React = window.__torchsnap.React;
 *
 * Future expansion: `components` and `keybindings` will be added
 * once barrel exports exist for `@torchsnap/components` and
 * `@torchsnap/keybindings`.
 */

import React from "react";

// =========================================================
// Type declaration
// =========================================================

declare global {
  interface Window {
    __torchsnap?: {
      React: typeof React;
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
  };
}
