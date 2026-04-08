// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Launcher entry barrel
//
// Re-exports every component the launcher webview loads
// from this plugin so vite's lib mode can produce a single
// `launcher.js` bundle with both named exports. The names
// here must match the manifest's `[frontend.views]` /
// `[frontend.inline-views]` mappings:
//
//   [frontend.views]
//   history = "CalculatorView"
//
//   [frontend.inline-views]
//   result = "CalculatorInline"
//
// Importing the launcher CSS from this barrel ensures it
// gets bundled into `launcher.css` once, rather than per
// view.
// =========================================================

import "../styles/launcher.css";

export { CalculatorView } from "./views/CalculatorView";
export { CalculatorInline } from "./views/CalculatorInline";
