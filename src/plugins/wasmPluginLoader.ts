// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Registers WASM plugin frontend components in the dynamic registry.
 *
 * For each WASM plugin, this module:
 * - Creates dynamic `import()` factories for views, inline views,
 *   and settings components served via `torchsnap-plugin://`
 * - Injects scoped CSS if declared in the manifest
 * - Registers the plugin with metadata (icon, description) so it
 *   appears in the settings sidebar with the generic wrapper
 *
 * All WASM plugins get a settings entry (for enable/disable at
 * minimum). Plugins with a `[frontend]` section additionally get
 * their view components registered.
 */

import type { ComponentType } from "react";
import { launcherComponent, settingsComponent } from "../lib/pluginComponent";
import { injectPluginCss } from "../lib/pluginCss";
import { registerPlugin, type PluginRegistryEntry } from "./registry";
import type { WasmPluginManifest } from "../lib/command";
import type { PluginViewProps, InlineViewProps } from "./types";

// =========================================================
// WASM Plugin Registration
// =========================================================

export type WebviewContext = "launcher" | "settings";

/**
 * Register a single WASM plugin based on its manifest data.
 * The `webview` parameter controls which CSS bundle is injected —
 * only the CSS relevant to the current webview is loaded.
 */
export function registerWasmPlugin(manifest: WasmPluginManifest, webview: WebviewContext): void {
  const pluginId = manifest.plugin.id;
  const baseUrl = `torchsnap-plugin://localhost/${pluginId}`;

  // Every WASM plugin gets metadata for the settings sidebar.
  // The PluginSettingsWrapper uses icon + description to render
  // the standardized header and enable/disable toggle.
  const entry: PluginRegistryEntry = {
    label: manifest.plugin.name,
    description: manifest.plugin.description,
    icon: manifest.plugin.icon,
  };

  const frontend = manifest.frontend;

  if (frontend) {
    // Launcher views (CustomUI).
    if (frontend.launcherBundle && Object.keys(frontend.views).length > 0) {
      const bundleUrl = `${baseUrl}/${frontend.launcherBundle}`;
      const views: Record<string, ComponentType<PluginViewProps>> = {};

      for (const [viewName, exportName] of Object.entries(frontend.views)) {
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
    if (frontend.launcherBundle && Object.keys(frontend.inlineViews).length > 0) {
      const bundleUrl = `${baseUrl}/${frontend.launcherBundle}`;
      const inlineViews: Record<string, ComponentType<InlineViewProps>> = {};

      for (const [viewName, exportName] of Object.entries(frontend.inlineViews)) {
        inlineViews[viewName] = launcherComponent(
          () =>
            import(/* @vite-ignore */ bundleUrl).then((mod) => ({
              default: mod[exportName],
            })),
        );
      }

      entry.inlineViews = inlineViews;
    }

    // Custom settings component.
    if (frontend.settingsBundle && frontend.settings?.component) {
      const bundleUrl = `${baseUrl}/${frontend.settingsBundle}`;
      const exportName = frontend.settings.component;

      entry.settings = settingsComponent(
        () =>
          import(/* @vite-ignore */ bundleUrl).then((mod) => ({
            default: mod[exportName],
          })),
      );
    }

    // Inject scoped CSS for the current webview only.
    // Fire-and-forget — CSS loading should not block registration.
    if (webview === "launcher" && frontend.launcherCss) {
      injectPluginCss(pluginId, frontend.launcherCss);
    }
    if (webview === "settings" && frontend.settingsCss) {
      injectPluginCss(pluginId, frontend.settingsCss);
    }
  }

  registerPlugin(pluginId, entry);
}

/**
 * Register all WASM plugins from the backend's manifest data.
 * Called once per webview at startup.
 */
export function registerAllWasmPlugins(
  manifests: WasmPluginManifest[],
  webview: WebviewContext,
): void {
  for (const manifest of manifests) {
    registerWasmPlugin(manifest, webview);
  }
}
