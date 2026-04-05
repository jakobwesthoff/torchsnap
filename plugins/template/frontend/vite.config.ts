// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { resolve } from "path";

// =========================================================
// Vite Configuration for a Torchsnap WASM Plugin
//
// Builds the plugin's frontend as ES module bundles that the
// host dynamically imports via `torchsnap-plugin://` protocol.
//
// Key design decisions:
//
// - React is NOT bundled. The `resolve.alias` entries redirect
//   `react` and `react/jsx-runtime` to shim files that
//   re-export from `window.__torchsnap`, where the host
//   provides its React instance at runtime.
//
// - Type imports (`@torchsnap/types`, `@torchsnap/plugin`)
//   point at the host source for TypeScript resolution but
//   are erased at compile time — no runtime dependency.
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

const sdkDir = resolve(__dirname, "../../../plugin-sdk");
const srcDir = resolve(__dirname, "../../../src");

// Determine which entry point to build. The build script sets
// PLUGIN_ENTRY to "launcher" or "settings".
const entryName = process.env.PLUGIN_ENTRY ?? "launcher";

const entries: Record<string, string> = {
  launcher: resolve(__dirname, "src/views/DemoView.tsx"),
  settings: resolve(__dirname, "src/settings/DemoSettings.tsx"),
};

const entry = entries[entryName];
if (!entry) {
  throw new Error(`Unknown PLUGIN_ENTRY: ${entryName}. Expected one of: ${Object.keys(entries).join(", ")}`);
}

export default defineConfig({
  plugins: [react(), tailwindcss()],

  resolve: {
    alias: {
      // Runtime: React comes from the host's window.__torchsnap global.
      // The jsx-runtime alias must come before the react alias so
      // rolldown matches "react/jsx-runtime" specifically instead of
      // treating it as a subpath of the "react" alias.
      "react/jsx-runtime": resolve(sdkDir, "shims/jsx-runtime.ts"),
      "react": resolve(sdkDir, "shims/react.ts"),

      // Types only (erased at compile time).
      "@torchsnap/types": resolve(srcDir, "types"),
      "@torchsnap/plugin": resolve(srcDir, "plugins/types.ts"),
    },
  },

  build: {
    lib: {
      entry: { [entryName]: entry },
      formats: ["es"],
    },
    // Single entry per build — no code splitting needed.
    rolldownOptions: {
      output: {
        codeSplitting: false,
        // Name the CSS file after the entry point so the manifest
        // can reference launcher.css and settings.css distinctly.
        assetFileNames: `${entryName}.[ext]`,
      },
    },
    cssCodeSplit: false,
    outDir: "dist",
    // Only clear dist on the first build (launcher). The settings
    // build appends to the existing dist directory.
    emptyOutDir: entryName === "launcher",
  },
});
