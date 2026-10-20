// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Gadget CSS scope in tests
//
// jsdom parses `@scope` rules but never applies them, so a test
// cannot ask whether a gadget's style reached an element. It can
// ask which elements the scope selector matches: this helper
// injects a gadget's CSS through the real `injectGadgetCss` and
// returns the selector of the `@scope` rule it wrapped the CSS in.
// Tests call `vi.unstubAllGlobals()` and `removeGadgetCss` after.
// =========================================================

import { vi } from "vitest";
import { injectGadgetCss } from "../lib/gadgetCss";

export async function injectedScopeSelector(gadgetId: string): Promise<string> {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => ({ ok: true, text: async () => ".probe { color: red; }" })),
  );
  await injectGadgetCss(gadgetId, "settings.css");

  const style = document.head.querySelector(`style[data-gadget-css="${gadgetId}"]`);
  const selector = style?.textContent?.match(/^@scope \((.+?)\) \{/)?.[1];
  if (!selector) {
    throw new Error(`no @scope rule injected for gadget ${gadgetId}`);
  }
  return selector;
}
