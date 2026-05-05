// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { torchsnap } from "@torchsnap/gadget-sdk/vite";
import { resolve } from "path";

// =========================================================
// Vite Configuration for a Torchsnap WASM Plugin
//
// Builds the plugin's frontend as ES module bundles that the
// host dynamically imports via `torchsnap-plugin://` protocol.
//
// Key design decisions:
//
// - React is NOT bundled. The `torchsnap()` Vite plugin
//   redirects `react` and `react/jsx-runtime` to SDK shims
//   that re-export from `window.__torchsnap`, where the host
//   provides its React instance at runtime.
//
// - Type imports from `@torchsnap/gadget-sdk` resolve through
//   node_modules (via the `link:` dependency) and are erased
//   at compile time — no runtime dependency.
//
// - Lib mode with `formats: ["es"]` produces ES modules with
//   named exports matching the manifest's view/component map.
//
// - Each entry point (launcher, settings) is built as a
//   separate self-contained bundle with no shared chunks.
//   The host loads them independently from different webviews
//   via dynamic import(), so sharing modules between them
//   is not possible.
//
// The build script (`bun run build`) invokes Vite once per
// entry point with the PLUGIN_ENTRY env var to select which
// entry to build.
// =========================================================

// ZeroTier ships only a settings panel — the plugin surfaces
// results through the standard `ScoredEntry` path with no
// custom launcher view, so there is one entry point.
const entryName = "settings";
const entry = resolve(__dirname, "src/settings/ZeroTierSettings.tsx");

export default defineConfig({
  plugins: [torchsnap(), react(), tailwindcss()],

  build: {
    lib: {
      entry: { [entryName]: entry },
      formats: ["es"],
    },
    rolldownOptions: {
      output: {
        codeSplitting: false,
        // Settings CSS distinct from any future launcher CSS.
        assetFileNames: `${entryName}.[ext]`,
      },
    },
    cssCodeSplit: false,
    outDir: "dist",
    emptyOutDir: true,
  },
});
