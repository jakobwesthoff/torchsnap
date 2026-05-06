// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Dynamic gadget registry mapping gadget IDs to their React components.
 *
 * Internal gadgets register at module load time; WASM gadgets register
 * at startup via `registerWasmGadget()` after querying the backend for
 * loaded gadget manifests.
 */

import { type ComponentType } from "react";
import { launcherComponent, settingsComponent } from "../lib/gadgetComponent";
import type { GadgetViewProps, GadgetSettingsProps, InlineViewProps } from "./types";

// =========================================================
// Registry shape
// =========================================================

export interface GadgetRegistryEntry {
  /** Human-readable name shown in the settings sidebar. */
  label: string;
  /** Short description shown in the settings section header. */
  description?: string;
  /** String icon identifier (e.g. "heroicons:clipboard-document-list").
   * Used for both the settings section header and the sidebar. */
  icon?: string;
  /** Named view components for the launcher result area (CustomUI). */
  views?: Record<string, ComponentType<GadgetViewProps>>;
  /** Named inline view components rendered above the result list (InlineUI). */
  inlineViews?: Record<string, ComponentType<InlineViewProps>>;
  /** Settings component rendered in the settings sidebar. */
  settings?: ComponentType<GadgetSettingsProps>;
}

// =========================================================
// Dynamic registry
// =========================================================

const registry = new Map<string, GadgetRegistryEntry>();

/**
 * Register a gadget in the registry. Overwrites any existing
 * entry for the same ID.
 */
export function registerGadget(id: string, entry: GadgetRegistryEntry): void {
  registry.set(id, entry);
}

/**
 * Remove a gadget from the registry (e.g., on uninstall).
 */
export function unregisterGadget(id: string): void {
  registry.delete(id);
}

// =========================================================
// Internal gadget registrations
//
// Commands and system_commands are intentionally excluded
// (internal, always-on).
// =========================================================

registerGadget("app-launcher", {
  label: "App Launcher",
  description: "Search and launch installed applications",
  icon: "heroicons:magnifying-glass",
});

registerGadget("system-preferences", {
  label: "System Settings",
  description: "Search and open macOS System Settings panes",
  icon: "heroicons:cog-8-tooth",
});

registerGadget("clipboard-manager", {
  label: "Clipboard",
  description: "Clipboard history with search and paste",
  icon: "heroicons:clipboard-document-list",
  views: {
    history: launcherComponent(() => import("./clipboard/ClipboardView")),
  },
  settings: settingsComponent(() => import("./clipboard/ClipboardSettings")),
});

// =========================================================
// Public API
// =========================================================

/**
 * Look up a named view component for a gadget's CustomUI response.
 * The view name is always provided — if missing, it's a bug.
 */
export function getGadgetView(
  gadgetId: string,
  viewName: string,
): ComponentType<GadgetViewProps> | undefined {
  return registry.get(gadgetId)?.views?.[viewName];
}

/**
 * Look up a named inline view component for a gadget's InlineUI response.
 */
export function getGadgetInlineView(
  gadgetId: string,
  viewName: string,
): ComponentType<InlineViewProps> | undefined {
  return registry.get(gadgetId)?.inlineViews?.[viewName];
}

/**
 * Look up the settings component for a gadget.
 */
export function getGadgetSettingsComponent(
  gadgetId: string,
): ComponentType<GadgetSettingsProps> | undefined {
  return registry.get(gadgetId)?.settings;
}

/**
 * Return all gadgets that have settings metadata, for building
 * the settings sidebar navigation. A gadget appears in settings
 * if it has either a custom settings component or at minimum an
 * icon + description (for the generic enable/disable wrapper).
 */
export function getGadgetsWithSettings(): Array<{
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
    // A gadget appears in settings if it has a custom settings
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
