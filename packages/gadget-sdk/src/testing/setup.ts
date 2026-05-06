// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// setupSdkGlobalsForTesting
//
// Populates `window.__torchsnap` with the host's React, JSX
// runtime, components, hooks, and keybinding implementations
// so that gadget component tests can resolve the
// `@torchsnap/gadget-sdk/...` shims at runtime.
//
// Call this once in a test setup file (e.g.
// `vitest.config.ts setupFiles` or a `beforeAll` hook).
//
// In-monorepo this delegates to the host's `initGadgetSdk()`
// via a path-based import so the test SDK and the production
// SDK are always in sync. When the SDK is eventually
// published to npm, this file will pivot to a vendored copy
// of the host implementations or to a separate published
// `@torchsnap/host-fixtures` package.
// =========================================================

// Path-based import into host source — works in-monorepo,
// breaks on npm publish (which is explicitly out of scope per
// the migration plan).
import { initGadgetSdk } from "../../../src/lib/sdk";

export function setupSdkGlobalsForTesting(): void {
  initGadgetSdk();
}
