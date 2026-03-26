// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Static map from plugin ID to React component.
 *
 * When the backend signals a plugin with custom UI (via
 * activePlugin in SearchMessage), the host looks up the component
 * here. Internal plugins register directly; this will be replaced
 * with dynamic resolution once plugin files and dynamic loading
 * are designed (see dynamic-plugin-component-registration todo).
 */

import { lazy, type ComponentType } from "react";
import type { PluginViewProps } from "./types";

// Lazy-load plugin components so they don't bloat the initial bundle.
const PLUGIN_COMPONENTS: Record<string, ComponentType<PluginViewProps>> = {
  "emoji-picker": lazy(() => import("./emoji/EmojiGrid")),
};

export function getPluginComponent(
  pluginId: string,
): ComponentType<PluginViewProps> | undefined {
  return PLUGIN_COMPONENTS[pluginId];
}
