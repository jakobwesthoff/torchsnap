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

interface PluginRegistryEntry {
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

const PLUGIN_REGISTRY: Record<string, PluginRegistryEntry> = {
  "emoji-picker": {
    label: "Emoji Picker",
    views: {
      picker: launcherComponent(() => import("./emoji/EmojiGrid")),
    },
  },
  "clipboard-manager": {
    label: "Clipboard",
    settingsIcon: ClipboardDocumentListIcon,
    views: {
      history: launcherComponent(() => import("./clipboard/ClipboardView")),
    },
    settings: settingsComponent(() => import("./clipboard/ClipboardSettings")),
  },
  bangs: {
    label: "Bangs",
    settingsIcon: ArrowTopRightOnSquareIcon,
    settings: settingsComponent(() => import("./bangs/BangsSettings")),
  },
  calculator: {
    label: "Calculator",
    settingsIcon: CalculatorIcon,
    views: {
      history: launcherComponent(() => import("./calculator/CalculatorView")),
    },
    inlineViews: {
      result: launcherComponent(() => import("./calculator/CalculatorInline")),
    },
    settings: settingsComponent(() => import("./calculator/CalculatorSettings")),
  },
};

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
  return PLUGIN_REGISTRY[pluginId]?.views?.[viewName];
}

/**
 * Look up a named inline view component for a plugin's InlineUI response.
 */
export function getPluginInlineView(
  pluginId: string,
  viewName: string,
): ComponentType<InlineViewProps> | undefined {
  return PLUGIN_REGISTRY[pluginId]?.inlineViews?.[viewName];
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
