// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// React Shim for WASM Plugins
//
// Bridges `import ... from "react"` in plugin TSX code to
// the host-provided React instance on `window.__torchsnap`.
//
// At build time, the plugin's Vite config aliases "react" to
// this file. The shim is inlined into the bundle (~2 lines
// of runtime code), so the output is a self-contained ES
// module with no unresolved imports.
//
// At runtime, the host calls `initGadgetSdk()` before any
// plugin code loads, ensuring `window.__torchsnap.React` is
// available.
// =========================================================

const React = window.__torchsnap!.React;
export default React;

// Named exports for hooks, utilities, and component helpers.
// Plugin code uses these via `import { useState } from "react"`.
export const {
  // Hooks
  useState,
  useEffect,
  useCallback,
  useMemo,
  useRef,
  useContext,
  useReducer,
  useSyncExternalStore,
  useId,

  // Component helpers
  createContext,
  forwardRef,
  memo,
  lazy,

  // Elements
  createElement,
  cloneElement,
  isValidElement,

  // Components / sentinels
  Suspense,
  Fragment,

  // Utilities
  Children,
} = React;
