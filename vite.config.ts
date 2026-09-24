// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { resolve } from "path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { guardedPagesCsp } from "./vite/csp";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react(), tailwindcss(), guardedPagesCsp(["update.html"])],

  // Stable import paths for gadget SDK surface. Must be kept in
  // sync with tsconfig.json paths.
  resolve: {
    alias: {
      "@torchsnap/keybindings": resolve(__dirname, "src/keybindings"),
      "@torchsnap/components": resolve(__dirname, "src/components"),
      "@torchsnap/types": resolve(__dirname, "src/types"),
    },
  },

  build: {
    rolldownOptions: {
      input: {
        main: resolve(__dirname, "launcher.html"),
        settings: resolve(__dirname, "settings.html"),
        devtools: resolve(__dirname, "devtools.html"),
        update: resolve(__dirname, "update.html"),
      },
      output: {
        manualChunks(id: string) {
          // Keep entry chunks free of shared exports. Rolldown may
          // otherwise inline shared modules into an entry chunk and
          // re-export them. When the other entry's lazy chunks import
          // from that chunk, the entry's bootstrap side effects
          // (mounting React, calling Tauri IPC) run in the wrong
          // window context. Extracting everything except the entry
          // points and lazy-loaded gadget components into a shared
          // chunk avoids this entirely.
          if (
            id.includes("/src/") &&
            !id.endsWith("/src/launcher/main.tsx") &&
            !id.endsWith("/src/settings/main.tsx") &&
            !id.endsWith("/src/devtools/main.tsx") &&
            !id.endsWith("/src/update/main.tsx") &&
            !id.includes("/src/gadgets/")
          ) {
            return "shared";
          }
        },
      },
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
