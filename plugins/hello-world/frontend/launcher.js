// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Hello World — Echo View
//
// Minimal frontend component demonstrating the WASM plugin
// dynamic loading pipeline. Uses React via the host SDK
// global (no build step, no JSX).
//
// Triggered by the "!" prefix. Displays the typed query
// as an echo, proving that:
//   1. torchsnap-plugin:// serves JS from the plugin source
//   2. dynamic import() resolves the named export
//   3. React from window.__torchsnap renders the component
//   4. CSS tokens from the host theme are accessible
// =========================================================

const { createElement: h } = window.__torchsnap.React;

/**
 * Echo view component. Receives the query typed after the
 * "!" prefix and displays it.
 *
 * @param {object} props - PluginViewProps from the host
 * @param {unknown} props.data - JSON data from the WASM search response
 * @param {string} props.query - Raw query string
 * @param {function} props.dismiss - Callback to close the launcher
 */
export function Echo({ data, query, dismiss }) {
  const echoText = data?.query ?? query ?? "";

  return h("div", { className: "echo-view" },
    h("p", { className: "echo-label" }, "Hello from WASM plugin!"),
    h("p", { className: "echo-query" }, echoText || "Type something after !"),
    h("button", {
      className: "echo-dismiss",
      onClick: dismiss,
    }, "Dismiss"),
  );
}
