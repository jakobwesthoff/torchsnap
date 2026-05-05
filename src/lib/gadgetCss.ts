// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Scoped CSS injection for WASM plugin frontend components.
 *
 * Fetches a plugin's CSS file via `torchsnap-plugin://` and wraps
 * it in a CSS `@scope` rule targeting the plugin's container
 * `[data-plugin="<plugin-id>"]`. This ensures plugin styles cannot
 * leak to the host or other plugins, while host styles (resets,
 * typography, design tokens) still cascade into the plugin.
 */

// =========================================================
// CSS Injection
// =========================================================

/**
 * Fetch a plugin's CSS and inject it as a scoped `<style>` element
 * in `<head>`. The CSS is wrapped in `@scope` so it only applies
 * inside the plugin's container div.
 */
export async function injectGadgetCss(gadgetId: string, cssPath: string): Promise<void> {
  const url = `torchsnap-plugin://localhost/${gadgetId}/${cssPath}`;
  const response = await fetch(url);

  if (!response.ok) {
    console.warn(
      `Failed to load CSS for plugin "${gadgetId}": ${response.status} ${response.statusText}`,
    );
    return;
  }

  const raw = await response.text();
  const scoped = `@scope ([data-plugin="${gadgetId}"]) {\n${raw}\n}`;

  const style = document.createElement("style");
  style.dataset.pluginCss = gadgetId;
  style.textContent = scoped;
  document.head.appendChild(style);
}

/**
 * Remove a previously injected plugin CSS `<style>` element.
 * Used when a plugin is uninstalled or disabled.
 */
export function removeGadgetCss(gadgetId: string): void {
  const style = document.head.querySelector(`style[data-plugin-css="${gadgetId}"]`);
  style?.remove();
}
