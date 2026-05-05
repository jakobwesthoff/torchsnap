// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Host Utilities Shim
//
// Bridges plugin code's
// `import { highlightText } from "@torchsnap/gadget-sdk/utils"`
// to the host-provided utility implementations on
// `window.__torchsnap.utils`.
//
// The utilities routed through this shim are ones whose
// semantics are a host-owned invariant — `highlightText`'s
// positions format (UTF-16 offsets into title/subtitle) is
// produced by the Rust backend and must stay in lockstep
// with the host's rendering logic. Shimming keeps the
// single source of truth on the host so a future change to
// the positions format updates every plugin at once.
// =========================================================

import "./global";
import type { ReactNode } from "react";

// ---------------------------------------------------------
// Ambient global
//
// The `TorchsnapGlobal` interface is declared piecemeal
// across the SDK shims and merged by TypeScript (see
// `hooks.ts` for the full explanation).
// ---------------------------------------------------------

declare global {
  interface TorchsnapGlobal {
    utils: {
      highlightText: (
        text: string,
        positions: readonly number[],
        highlightClassName?: string,
      ) => ReactNode[];
    };
  }
}

// ---------------------------------------------------------
// Re-exports
//
// Host lookup is deferred to call-time so this module is
// safe to evaluate before `initGadgetSdk()` has populated
// the global.
// ---------------------------------------------------------

function hostUtils() {
  const t = window.__torchsnap;
  if (!t) {
    throw new Error(
      "@torchsnap/gadget-sdk/utils: window.__torchsnap is not initialized — call initGadgetSdk() before loading plugin bundles",
    );
  }
  return t.utils;
}

/**
 * Split `text` at the given UTF-16 offsets and wrap each
 * matched run in a highlighted span. The positions format
 * matches what the Rust backend emits through
 * `ScoredEntry.title_highlight_positions` and
 * `ScoredEntry.subtitle_highlight_positions` — i.e. UTF-16
 * code unit offsets, matching JavaScript's native string
 * indexing.
 *
 * `highlightClassName` defaults to the host's standard
 * accent styling when omitted; plugins that want a custom
 * look (e.g. underlined monochrome in a footer) pass their
 * own tailwind classes.
 */
export function highlightText(
  text: string,
  positions: readonly number[],
  highlightClassName?: string,
): ReactNode[] {
  return hostUtils().highlightText(text, positions, highlightClassName);
}
