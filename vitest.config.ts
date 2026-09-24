// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { defineConfig, mergeConfig } from "vitest/config";
import viteConfig from "./vite.config";

// The Vite config is an async factory, so it is resolved here and
// merged rather than duplicated. That keeps the `@torchsnap/*` aliases
// and the React plugin in exactly one place.
export default defineConfig(async (env) =>
  mergeConfig(await viteConfig(env), {
    test: {
      environment: "jsdom",
      include: ["src/**/*.test.{ts,tsx}"],
      setupFiles: ["src/test/setup.ts"],
    },
  }),
);
