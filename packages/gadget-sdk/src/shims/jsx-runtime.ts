// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// JSX Runtime Shim for WASM Gadgets
//
// Bridges `react/jsx-runtime` (used by the automatic JSX
// transform) to the host-provided runtime on
// `window.__torchsnap.jsxRuntime`.
//
// The host exposes `import * as jsxRuntime from "react/jsx-runtime"`
// on the global. This shim re-exports `jsx`, `jsxs`, and
// `Fragment` so that Vite's JSX transform output resolves
// correctly at runtime.
// =========================================================

const runtime = window.__torchsnap!.jsxRuntime;

export const { jsx, jsxs, Fragment } = runtime;
