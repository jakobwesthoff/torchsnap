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

import { type ComponentType, type SVGProps } from "react";
import {
  ArrowTopRightOnSquareIcon,
  CalculatorIcon,
  ClipboardDocumentListIcon,
} from "@heroicons/react/24/outline";
import { launcherComponent, settingsComponent } from "../lib/pluginComponent";
import type { PluginViewProps, PluginSettingsProps, InlineViewProps } from "./types";

// =========================================================
// Registry shape
// =========================================================

export interface PluginRegistryEntry {
  /** Human-readable name shown in the settings sidebar. */
  label: string;
  /** Icon shown next to the label in the settings sidebar. */
  settingsIcon?: ComponentType<SVGProps<SVGSVGElement>>;
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
// =========================================================

registerPlugin("emoji-picker", {
  label: "Emoji Picker",
  views: {
    picker: launcherComponent(() => import("./emoji/EmojiGrid")),
  },
});

registerPlugin("clipboard-manager", {
  label: "Clipboard",
  settingsIcon: ClipboardDocumentListIcon,
  views: {
    history: launcherComponent(() => import("./clipboard/ClipboardView")),
  },
  settings: settingsComponent(() => import("./clipboard/ClipboardSettings")),
});

registerPlugin("bangs", {
  label: "Bangs",
  settingsIcon: ArrowTopRightOnSquareIcon,
  settings: settingsComponent(() => import("./bangs/BangsSettings")),
});

registerPlugin("calculator", {
  label: "Calculator",
  settingsIcon: CalculatorIcon,
  views: {
    history: launcherComponent(() => import("./calculator/CalculatorView")),
  },
  inlineViews: {
    result: launcherComponent(() => import("./calculator/CalculatorInline")),
  },
  settings: settingsComponent(() => import("./calculator/CalculatorSettings")),
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
 * Return all plugins that have a settings component, for building
 * the settings sidebar navigation.
 */
export function getPluginsWithSettings(): Array<{
  id: string;
  label: string;
  settingsIcon?: ComponentType<SVGProps<SVGSVGElement>>;
}> {
  const result: Array<{
    id: string;
    label: string;
    settingsIcon?: ComponentType<SVGProps<SVGSVGElement>>;
  }> = [];

  for (const [id, entry] of registry) {
    if (entry.settings != null) {
      result.push({
        id,
        label: entry.label,
        settingsIcon: entry.settingsIcon,
      });
    }
  }

  return result;
}
