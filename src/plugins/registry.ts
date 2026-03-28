// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Static registry mapping plugin IDs to their React components.
 *
 * Each plugin can register up to two components:
 * - `view`: the custom UI shown in the launcher result area (ADR 0013)
 * - `settings`: the settings panel shown in the settings sidebar
 *
 * Internal plugins register directly here. This will be replaced
 * with dynamic resolution once plugin files and dynamic loading
 * are designed (see dynamic-plugin-component-registration todo).
 */

import { lazy, type ComponentType, type SVGProps } from "react";
import { ClipboardDocumentListIcon } from "@heroicons/react/24/outline";
import type { PluginViewProps, PluginSettingsProps } from "./types";

// =========================================================
// Registry shape
// =========================================================

interface PluginRegistryEntry {
  /** Human-readable name shown in the settings sidebar. */
  label: string;
  /** Icon shown next to the label in the settings sidebar. */
  settingsIcon?: ComponentType<SVGProps<SVGSVGElement>>;
  /** Custom UI component for the launcher result area. */
  view?: ComponentType<PluginViewProps>;
  /** Settings component rendered in the settings sidebar. */
  settings?: ComponentType<PluginSettingsProps>;
}

// Lazy-load plugin components so they don't bloat the initial bundle.
const PLUGIN_REGISTRY: Record<string, PluginRegistryEntry> = {
  "emoji-picker": {
    label: "Emoji Picker",
    view: lazy(() => import("./emoji/EmojiGrid")),
  },
  "clipboard-manager": {
    label: "Clipboard",
    settingsIcon: ClipboardDocumentListIcon,
    view: lazy(() => import("./clipboard/ClipboardView")),
    settings: lazy(() => import("./clipboard/ClipboardSettings")),
  },
};

// =========================================================
// Public API
// =========================================================

/**
 * Look up the custom UI (launcher view) component for a plugin.
 */
export function getPluginComponent(pluginId: string): ComponentType<PluginViewProps> | undefined {
  return PLUGIN_REGISTRY[pluginId]?.view;
}

/**
 * Look up the settings component for a plugin.
 */
export function getPluginSettingsComponent(
  pluginId: string,
): ComponentType<PluginSettingsProps> | undefined {
  return PLUGIN_REGISTRY[pluginId]?.settings;
}

/**
 * Return all plugins that have a settings component, for building
 * the settings sidebar navigation.
 */
export function getPluginsWithSettings(): Array<{
  id: string;
  label: string;
  settingsIcon?: ComponentType<SVGProps<SVGSVGElement>>;
}> {
  return Object.entries(PLUGIN_REGISTRY)
    .filter(([, entry]) => entry.settings != null)
    .map(([id, entry]) => ({
      id,
      label: entry.label,
      settingsIcon: entry.settingsIcon,
    }));
}
