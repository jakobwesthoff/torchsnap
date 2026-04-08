// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Dynamic plugin registry mapping plugin IDs to their React components.
 *
 * Supports both internal (compile-time) and external (runtime) plugins.
 * Internal plugins register at module load time; WASM plugins register
 * at startup via `registerWasmPlugin()` after querying the backend for
 * loaded plugin manifests.
 *
 * The public lookup API is unchanged from the previous static registry:
 * `getPluginView`, `getPluginInlineView`, `getPluginSettingsComponent`,
 * `getPluginsWithSettings`.
 */

import { type ComponentType } from "react";
import { launcherComponent, settingsComponent } from "../lib/pluginComponent";
import type { PluginViewProps, PluginSettingsProps, InlineViewProps } from "./types";

// =========================================================
// Registry shape
// =========================================================

export interface PluginRegistryEntry {
  /** Human-readable name shown in the settings sidebar. */
  label: string;
  /** Short description shown in the settings section header. */
  description?: string;
  /** String icon identifier (e.g. "heroicons:clipboard-document-list").
   * Used for both the settings section header and the sidebar. */
  icon?: string;
  /** Named view components for the launcher result area (CustomUI). */
  views?: Record<string, ComponentType<PluginViewProps>>;
  /** Named inline view components rendered above the result list (InlineUI). */
  inlineViews?: Record<string, ComponentType<InlineViewProps>>;
  /** Settings component rendered in the settings sidebar. */
  settings?: ComponentType<PluginSettingsProps>;
}

// =========================================================
// Dynamic registry
// =========================================================

const registry = new Map<string, PluginRegistryEntry>();

/**
 * Register a plugin in the registry. Overwrites any existing
 * entry for the same ID.
 */
export function registerPlugin(id: string, entry: PluginRegistryEntry): void {
  registry.set(id, entry);
}

/**
 * Remove a plugin from the registry (e.g., on uninstall).
 */
export function unregisterPlugin(id: string): void {
  registry.delete(id);
}

// =========================================================
// Internal plugin registrations
//
// Plugins that previously had no settings entry now get one
// with just a label, icon, and description — the generic
// PluginSettingsWrapper provides the enable/disable toggle.
// Commands and system_commands are intentionally excluded
// (internal, always-on).
// =========================================================

registerPlugin("app-launcher", {
  label: "App Launcher",
  description: "Search and launch installed applications",
  icon: "heroicons:magnifying-glass",
});

registerPlugin("system-preferences", {
  label: "System Settings",
  description: "Search and open macOS System Settings panes",
  icon: "heroicons:cog-8-tooth",
});

registerPlugin("emoji-picker", {
  label: "Emoji Picker",
  description: "Search and insert emoji characters",
  icon: "heroicons:face-smile",
  views: {
    picker: launcherComponent(() => import("./emoji/EmojiGrid")),
  },
});

registerPlugin("clipboard-manager", {
  label: "Clipboard",
  description: "Clipboard history with search and paste",
  icon: "heroicons:clipboard-document-list",
  views: {
    history: launcherComponent(() => import("./clipboard/ClipboardView")),
  },
  settings: settingsComponent(() => import("./clipboard/ClipboardSettings")),
});

registerPlugin("bangs", {
  label: "Bangs",
  description: "DuckDuckGo bang shortcuts for quick web searches",
  icon: "heroicons:arrow-top-right-on-square",
  settings: settingsComponent(() => import("./bangs/BangsSettings")),
});

// `calculator` is now a WASM plugin loaded from
// `plugins/calculator/`. The host's `wasmPluginLoader`
// registers it dynamically at startup, so the static
// registry no longer needs an entry.

registerPlugin("open-url", {
  label: "Open URL",
  description: "Detect and open URLs typed in the search bar",
  icon: "heroicons:globe-alt",
});

// =========================================================
// Public API
// =========================================================

/**
 * Look up a named view component for a plugin's CustomUI response.
 * The view name is always provided — if missing, it's a bug.
 */
export function getPluginView(
  pluginId: string,
  viewName: string,
): ComponentType<PluginViewProps> | undefined {
  return registry.get(pluginId)?.views?.[viewName];
}

/**
 * Look up a named inline view component for a plugin's InlineUI response.
 */
export function getPluginInlineView(
  pluginId: string,
  viewName: string,
): ComponentType<InlineViewProps> | undefined {
  return registry.get(pluginId)?.inlineViews?.[viewName];
}

/**
 * Look up the settings component for a plugin.
 */
export function getPluginSettingsComponent(
  pluginId: string,
): ComponentType<PluginSettingsProps> | undefined {
  return registry.get(pluginId)?.settings;
}

/**
 * Return all plugins that have settings metadata, for building
 * the settings sidebar navigation. A plugin appears in settings
 * if it has either a custom settings component or at minimum an
 * icon + description (for the generic enable/disable wrapper).
 */
export function getPluginsWithSettings(): Array<{
  id: string;
  label: string;
  description?: string;
  icon?: string;
}> {
  const result: Array<{
    id: string;
    label: string;
    description?: string;
    icon?: string;
  }> = [];

  for (const [id, entry] of registry) {
    // A plugin appears in settings if it has a custom settings
    // component OR has metadata (icon + description) for the
    // generic wrapper.
    if (entry.settings != null || (entry.icon != null && entry.description != null)) {
      result.push({
        id,
        label: entry.label,
        description: entry.description,
        icon: entry.icon,
      });
    }
  }

  return result;
}
