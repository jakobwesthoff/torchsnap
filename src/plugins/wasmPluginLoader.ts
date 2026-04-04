// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Registers WASM plugin frontend components in the dynamic registry.
 *
 * For each WASM plugin that declares a `[frontend]` section in its
 * manifest, this module creates dynamic `import()` factories pointing
 * at `torchsnap-plugin://localhost/<plugin-id>/<bundle-path>` URLs.
 * The host serves these assets via the custom protocol handler.
 *
 * Named exports from the bundle are mapped to view names as declared
 * in the manifest's `[frontend.views]` and `[frontend.inline-views]`
 * tables.
 */

import type { ComponentType } from "react";
import { launcherComponent, settingsComponent } from "../lib/pluginComponent";
import { injectPluginCss } from "../lib/pluginCss";
import { registerPlugin, type PluginRegistryEntry } from "./registry";
import type { WasmFrontendInfo } from "../lib/command";
import type { PluginViewProps, InlineViewProps } from "./types";

// =========================================================
// WASM Plugin Registration
// =========================================================

/**
 * Register a single WASM plugin's frontend components based
 * on its manifest data.
 */
export function registerWasmPlugin(info: WasmFrontendInfo): void {
  const baseUrl = `torchsnap-plugin://localhost/${info.pluginId}`;

  const entry: PluginRegistryEntry = {
    label: info.name,
  };

  // Launcher views (CustomUI).
  if (info.launcherBundle && Object.keys(info.views).length > 0) {
    const bundleUrl = `${baseUrl}/${info.launcherBundle}`;
    const views: Record<string, ComponentType<PluginViewProps>> = {};

    for (const [viewName, exportName] of Object.entries(info.views)) {
      views[viewName] = launcherComponent(
        () =>
          import(/* @vite-ignore */ bundleUrl).then((mod) => ({
            default: mod[exportName],
          })),
      );
    }

    entry.views = views;
  }

  // Inline views (InlineUI).
  if (info.launcherBundle && Object.keys(info.inlineViews).length > 0) {
    const bundleUrl = `${baseUrl}/${info.launcherBundle}`;
    const inlineViews: Record<string, ComponentType<InlineViewProps>> = {};

    for (const [viewName, exportName] of Object.entries(info.inlineViews)) {
      inlineViews[viewName] = launcherComponent(
        () =>
          import(/* @vite-ignore */ bundleUrl).then((mod) => ({
            default: mod[exportName],
          })),
      );
    }

    entry.inlineViews = inlineViews;
  }

  // Settings component.
  if (info.settingsBundle && info.settingsComponent) {
    const bundleUrl = `${baseUrl}/${info.settingsBundle}`;
    const exportName = info.settingsComponent;

    entry.settings = settingsComponent(
      () =>
        import(/* @vite-ignore */ bundleUrl).then((mod) => ({
          default: mod[exportName],
        })),
    );
  }

  registerPlugin(info.pluginId, entry);

  // Inject scoped CSS if the plugin declares a launcher CSS file.
  // This is fire-and-forget — CSS loading should not block plugin
  // registration.
  if (info.launcherCss) {
    injectPluginCss(info.pluginId, info.launcherCss);
  }
}

/**
 * Register all WASM plugins from the backend's manifest data.
 * Called once per webview at startup.
 */
export function registerAllWasmPlugins(plugins: WasmFrontendInfo[]): void {
  for (const plugin of plugins) {
    registerWasmPlugin(plugin);
  }
}
