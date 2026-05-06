// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Scoped CSS injection for WASM gadget frontend components.
 *
 * Fetches a gadget's CSS file via `torchsnap-gadget://` and wraps
 * it in a CSS `@scope` rule targeting the gadget's container
 * `[data-gadget="<gadget-id>"]`. This ensures gadget styles cannot
 * leak to the host or other gadgets, while host styles (resets,
 * typography, design tokens) still cascade into the gadget.
 */

// =========================================================
// CSS Injection
// =========================================================

/**
 * Fetch a gadget's CSS and inject it as a scoped `<style>` element
 * in `<head>`. The CSS is wrapped in `@scope` so it only applies
 * inside the gadget's container div.
 */
export async function injectGadgetCss(gadgetId: string, cssPath: string): Promise<void> {
  const url = `torchsnap-gadget://localhost/${gadgetId}/${cssPath}`;
  const response = await fetch(url);

  if (!response.ok) {
    console.warn(
      `Failed to load CSS for gadget "${gadgetId}": ${response.status} ${response.statusText}`,
    );
    return;
  }

  const raw = await response.text();
  const scoped = `@scope ([data-gadget="${gadgetId}"]) {\n${raw}\n}`;

  const style = document.createElement("style");
  style.dataset.gadgetCss = gadgetId;
  style.textContent = scoped;
  document.head.appendChild(style);
}

/**
 * Remove a previously injected gadget CSS `<style>` element.
 * Used when a gadget is uninstalled or disabled.
 */
export function removeGadgetCss(gadgetId: string): void {
  const style = document.head.querySelector(`style[data-gadget-css="${gadgetId}"]`);
  style?.remove();
}
