// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Gadget component prop interfaces — host-side mirror of
 * `@torchsnap/gadget-sdk`'s `gadget.ts`.
 *
 * Per ADR 0028, ambient capabilities (`gadgetId`, `sendMessage`,
 * `logger`, launcher actions, reactive setting accessors) are
 * provided via the React context exposed by `src/contexts/`. The
 * three prop interfaces here carry only per-render data — the
 * pieces that change on every keystroke and would invalidate
 * the context value if hoisted.
 */

import type { SourcedEntry } from "@torchsnap/types";

export interface GadgetViewProps {
  /** Search results from the normal search() flow. The gadget
   *  decides whether to use them or ignore them. */
  results: SourcedEntry[];
  /** Opaque data from the backend's GadgetViewRef.data field.
   *  Only present when the gadget returned CustomUI or InlineUI
   *  with a data payload. */
  data?: unknown;
  /** Current query, stripped of the matched prefix. */
  query: string;
  /** Which prefix activated the gadget. */
  matchedPrefix: string;
}

// =========================================================
// Gadget View Reference
// =========================================================

/**
 * Reference to a gadget view component, sent from the backend.
 * The frontend resolves this to a React component via the gadget
 * registry: `registry[gadgetId].views[view]` for CustomUI,
 * `registry[gadgetId].inlineViews[view]` for InlineUI.
 */
export interface GadgetViewRef {
  gadgetId: string;
  view: string;
  data?: unknown;
}

// =========================================================
// Inline View Props
// =========================================================

/**
 * Props for inline view components (InlineUI). Rendered above the
 * standard result list, these participate in the host's selection
 * model at index 0.
 */
export interface InlineViewProps {
  /** Opaque data from the backend's `GadgetViewRef.data`. */
  data: unknown;
  /** Current search query (stripped of prefix). */
  query: string;
  /** Prefix that activated the gadget (empty in heuristic mode). */
  matchedPrefix: string;
  /** Whether the inline slot is currently selected (index 0). */
  selected: boolean;
}

// =========================================================
// Gadget Settings UI
// =========================================================

/**
 * Settings panels receive no per-render data. Identity, runtime
 * capabilities, and reactive setting accessors all come from the
 * gadget context hooks (`useGadgetInfo`, `useGadgetRuntime`,
 * `useGadgetSetting`). The interface stays as a named (empty)
 * type so `ComponentType<GadgetSettingsProps>` continues to
 * typecheck consistently with the SDK mirror.
 */
// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface GadgetSettingsProps {}
