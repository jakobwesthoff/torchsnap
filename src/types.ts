// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Shared frontend types.
 *
 * Types that mirror Rust structs/enums must stay in sync with their
 * backend counterparts. Each section notes the corresponding Rust
 * source file.
 */

import type { ReactNode } from "react";

// =========================================================
// Action Types
//
// Mirrors `src-tauri/src/search/types.rs`.
// Field names use camelCase (Rust types use
// `#[serde(rename_all = "camelCase")]`).
// =========================================================

export type ActionId =
  | { type: "open" }
  | { type: "copy" }
  | { type: "reveal" }
  | { type: "openWith" }
  | { type: "delete" }
  | { type: "custom"; value: string };

export interface ActionKeybinding {
  modifiers?: string[];
  key: string;
}

export interface Action {
  id: ActionId;
  label: string;
  keybinding: ActionKeybinding | null;
}

// =========================================================
// Post-Action Types
//
// Mirrors `PostAction` in `src-tauri/src/search/types.rs`.
//
// Returned by the `search_execute` Tauri command to tell the
// frontend what to do after a plugin handles an action.
// =========================================================

export type PostAction =
  | "Nothing"
  | "Dismiss"
  | "KeepOpen"
  | { ShowCustomUI: { view: string; data?: unknown } };

// =========================================================
// Entry Types
//
// Mirrors `src-tauri/src/search/types.rs`.
// =========================================================

export type EntryIcon =
  | { type: "heroIcon"; value: string }
  | { type: "dataUrl"; value: string }
  | { type: "assetIcon"; value: string }
  | { type: "emoji"; value: string };

export interface SourcedEntry {
  id: string;
  title: string;
  subtitle: string | null;
  icon: EntryIcon | null;
  score: number;
  titlePositions: number[];
  subtitlePositions: number[];
  source: string;
  actions: Action[];
}

// =========================================================
// Footer Types (frontend-only)
// =========================================================

export interface FooterHint {
  combo?: { modifiers: string[]; key: string };
  label: ReactNode;
}

export interface FooterState {
  primary?: FooterHint;
  hints: FooterHint[];
}

// =========================================================
// Plugin View Reference
//
// Mirrors `src-tauri/src/search/types.rs`.
// =========================================================

/** Reference to a gadget view component, sent from the backend. */
export interface GadgetViewRef {
  gadgetId: string;
  view: string;
  data?: unknown;
}

// =========================================================
// Channel Messages
//
// Mirrors `SearchMessage` in `src-tauri/src/search/mod.rs`.
// =========================================================

/**
 * Discriminated union mirroring the Rust `ResultSource` enum.
 * `Catalog` is one aggregated batch mixing rows from every
 * catalog-providing plugin — it replaces the catalog layer
 * wholesale, not per-plugin.
 */
export type ResultSource = { type: "gadget"; id: string } | { type: "catalog" };

/** Stable string key for `ResultSource`. Namespacing the
 *  `gadget:` prefix guards against a gadget id colliding with
 *  the catalog layer's key. */
export function resultSourceKey(source: ResultSource): string {
  return source.type === "catalog" ? "catalog" : `gadget:${source.id}`;
}

export type SearchMessage =
  | {
      type: "searchResults";
      source: ResultSource;
      entries: SourcedEntry[];
      /** View reference when the gadget requested custom UI. */
      customGadgetView: GadgetViewRef | null;
      /** View reference when the gadget requested inline UI. */
      inlineGadgetView: GadgetViewRef | null;
      /** The prefix that triggered exclusive routing (e.g., ":"). */
      matchedPrefix: string | null;
    }
  | { type: "done" };

// =========================================================
// Control Channel
//
// Mirrors `ControlCommand` in `src-tauri/src/control/mod.rs`.
// =========================================================

export type ControlCommand = { type: "dismiss" } | { type: "setQuery"; text: string };

// =========================================================
// Frecency
//
// Mirrors `FrecencyStats` in `src-tauri/src/frecency/mod.rs`.
// =========================================================

export interface FrecencyStats {
  totalEvents: number;
  uniqueItems: number;
  eventsByGadget: Record<string, number>;
  oldestEvent: number | null;
}
