// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Dynamic plugin registry mapping plugin IDs to their React components.
 *
 * Internal plugins register at module load time; WASM plugins register
 * at startup via `registerWasmPlugin()` after querying the backend for
 * loaded plugin manifests.
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

export function registerPlugin(id: string, entry: PluginRegistryEntry): void {
  registry.set(id, entry);
}

export function unregisterPlugin(id: string): void {
  registry.delete(id);
}

// =========================================================
// Internal plugin registrations
//
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

registerPlugin("open-url", {
  label: "Open URL",
  description: "Detect and open URLs typed in the search bar",
  icon: "heroicons:globe-alt",
});

// =========================================================
// Public API
// =========================================================

export function getPluginView(
  pluginId: string,
  viewName: string,
): ComponentType<PluginViewProps> | undefined {
  return registry.get(pluginId)?.views?.[viewName];
}

export function getPluginInlineView(
  pluginId: string,
  viewName: string,
): ComponentType<InlineViewProps> | undefined {
  return registry.get(pluginId)?.inlineViews?.[viewName];
}

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
