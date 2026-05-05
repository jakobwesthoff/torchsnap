// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Torchsnap Vite Plugin
//
// Configures the build aliases that every plugin needs:
//
// - `react` → SDK shim that re-exports from window.__torchsnap.React
// - `react/jsx-runtime` → SDK shim for the automatic JSX transform
//
// This keeps plugin vite configs clean — instead of manually
// wiring up resolve aliases to shim files, plugins just add
// `torchsnap()` to their plugin array:
//
//   import { torchsnap } from "@torchsnap/gadget-sdk/vite";
//
//   export default defineConfig({
//     plugins: [torchsnap(), react(), tailwindcss()],
//   });
// =========================================================

import { resolve, dirname } from "path";
import { fileURLToPath } from "url";
import type { Plugin } from "vite";

const __dirname = dirname(fileURLToPath(import.meta.url));

export function torchsnap(): Plugin {
  return {
    name: "torchsnap-plugin-sdk",
    config() {
      return {
        resolve: {
          alias: {
            // The jsx-runtime alias must come before the react alias so
            // the bundler matches "react/jsx-runtime" specifically instead
            // of treating it as a subpath of the "react" alias.
            "react/jsx-runtime": resolve(__dirname, "../shims/jsx-runtime.ts"),
            "react": resolve(__dirname, "../shims/react.ts"),
          },
        },
      };
    },
  };
}
