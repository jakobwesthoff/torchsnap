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
import { CalculatorIcon, ClipboardDocumentListIcon } from "@heroicons/react/24/outline";
import type { PluginViewProps, PluginSettingsProps, InlineViewProps } from "./types";

// =========================================================
// Registry shape
// =========================================================

type LazyComponent<P> = ComponentType<P>;

interface PluginRegistryEntry {
  /** Human-readable name shown in the settings sidebar. */
  label: string;
  /** Icon shown next to the label in the settings sidebar. */
  settingsIcon?: ComponentType<SVGProps<SVGSVGElement>>;
  /** Named view components for the launcher result area (CustomUI). */
  views?: Record<string, LazyComponent<PluginViewProps>>;
  /** Named inline view components rendered above the result list (InlineUI). */
  inlineViews?: Record<string, LazyComponent<InlineViewProps>>;
  /** Settings component rendered in the settings sidebar. */
  settings?: LazyComponent<PluginSettingsProps>;
}

// Lazy-load plugin components so they don't bloat the initial bundle.
const PLUGIN_REGISTRY: Record<string, PluginRegistryEntry> = {
  "emoji-picker": {
    label: "Emoji Picker",
    views: {
      picker: lazy(() => import("./emoji/EmojiGrid")),
    },
  },
  "clipboard-manager": {
    label: "Clipboard",
    settingsIcon: ClipboardDocumentListIcon,
    views: {
      // The clipboard plugin uses execute-triggered custom UI
      // (ShowCustomUI), so the view name is not sent by the backend.
      // The frontend activates it via `executePluginView` with just
      // the plugin ID. We register it under "default" and resolve
      // with a fallback in `getPluginView`.
      default: lazy(() => import("./clipboard/ClipboardView")),
    },
    settings: lazy(() => import("./clipboard/ClipboardSettings")),
  },
  calculator: {
    label: "Calculator",
    settingsIcon: CalculatorIcon,
    views: {
      history: lazy(() => import("./calculator/CalculatorView")),
    },
    inlineViews: {
      result: lazy(() => import("./calculator/CalculatorInline")),
    },
    settings: lazy(() => import("./calculator/CalculatorSettings")),
  },
};

// =========================================================
// Public API
// =========================================================

/**
 * Look up a named view component for a plugin's CustomUI response.
 * Falls back to "default" when no view name is specified (for
 * execute-triggered plugins that don't send a view name).
 */
export function getPluginView(
  pluginId: string,
  viewName?: string,
): LazyComponent<PluginViewProps> | undefined {
  const views = PLUGIN_REGISTRY[pluginId]?.views;
  if (!views) return undefined;
  return views[viewName ?? "default"] ?? views["default"];
}

/**
 * Look up a named inline view component for a plugin's InlineUI response.
 */
export function getPluginInlineView(
  pluginId: string,
  viewName: string,
): LazyComponent<InlineViewProps> | undefined {
  return PLUGIN_REGISTRY[pluginId]?.inlineViews?.[viewName];
}

/**
 * Look up the settings component for a plugin.
 */
export function getPluginSettingsComponent(
  pluginId: string,
): LazyComponent<PluginSettingsProps> | undefined {
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
