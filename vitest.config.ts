// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { resolve } from "path";
import { defineConfig, mergeConfig } from "vitest/config";
import viteConfig from "./vite.config";

// Gadget frontends import the SDK as `@torchsnap/gadget-sdk/...`,
// which each frontend's own `node_modules` holds as a copy. Tests
// resolve it to the in-repo package instead, so a gadget view and
// its test share one copy of the shims, and the SDK's testing
// helpers keep their relative imports into the host sources.
const gadgetSdk = resolve(import.meta.dirname, "packages/gadget-sdk/src");

// The Vite config is an async factory, so it is resolved here and
// merged rather than duplicated. That keeps the `@torchsnap/*` aliases
// and the React plugin in exactly one place.
export default defineConfig(async (env) =>
  mergeConfig(await viteConfig(env), {
    resolve: {
      alias: [
        { find: /^@torchsnap\/gadget-sdk$/, replacement: `${gadgetSdk}/types/index.ts` },
        { find: /^@torchsnap\/gadget-sdk\/hooks$/, replacement: `${gadgetSdk}/shims/hooks.ts` },
        {
          find: /^@torchsnap\/gadget-sdk\/components$/,
          replacement: `${gadgetSdk}/shims/components.ts`,
        },
        {
          find: /^@torchsnap\/gadget-sdk\/keybindings$/,
          replacement: `${gadgetSdk}/shims/keybindings.ts`,
        },
        { find: /^@torchsnap\/gadget-sdk\/utils$/, replacement: `${gadgetSdk}/shims/utils.ts` },
        { find: /^@torchsnap\/gadget-sdk\/testing$/, replacement: `${gadgetSdk}/testing/index.ts` },
      ],
    },
    test: {
      environment: "jsdom",
      include: [
        "src/**/*.test.{ts,tsx}",
        "vite/**/*.test.ts",
        "gadgets/*/frontend/src/**/*.test.{ts,tsx}",
      ],
      setupFiles: ["src/test/setup.ts"],
    },
  }),
);
