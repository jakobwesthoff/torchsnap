// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Gadget Component Props
//
// Per-render data passed to the three gadget component
// kinds. Everything else (identity, sendMessage, logger,
// launcher actions) flows through the React context exposed
// via `@torchsnap/gadget-sdk/hooks` so that sub-components
// don't need to thread props through every level.
//
// - GadgetViewProps     — full-screen launcher view
// - InlineViewProps     — inline result row above the list
// - GadgetSettingsProps — settings sidebar panel (no per-render data)
//
// The host constructs objects satisfying these interfaces
// before rendering gadget components and wraps the mount in
// a <GadgetContextProvider> that supplies everything else.
// =========================================================

import type { SourcedEntry } from "./data";

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

export interface InlineViewProps {
  /** Opaque data from the backend's GadgetViewRef.data field. */
  data: unknown;
  /** Current search query (stripped of prefix). */
  query: string;
  /** Prefix that activated the gadget (empty in heuristic mode). */
  matchedPrefix: string;
  /** Whether the inline slot is currently selected (index 0). */
  selected: boolean;
}

/**
 * Settings panels receive no per-render data. Identity, runtime
 * capabilities, and reactive setting accessors all come from the
 * gadget context hooks. Kept as a named (empty) interface so that
 * `ComponentType<GadgetSettingsProps>` continues to typecheck and
 * future per-render fields (if any) have an obvious home.
 */
// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface GadgetSettingsProps {}
