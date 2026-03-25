// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * TypeScript mirrors of the Rust search types.
 *
 * These must stay in sync with `src-tauri/src/search/types.rs`.
 * Field names use camelCase (Rust types use `#[serde(rename_all = "camelCase")]`).
 */

// =========================================================
// Action Types
// =========================================================

export type ActionId =
  | { type: "open" }
  | { type: "copy" }
  | { type: "reveal" }
  | { type: "openWith" }
  | { type: "delete" }
  | { type: "custom"; value: string };

export interface ActionKeybinding {
  label: string;
}

export interface Action {
  id: ActionId;
  label: string;
  keybinding: ActionKeybinding | null;
}

// =========================================================
// Entry Types
// =========================================================

export type EntryIcon =
  | { type: "heroIcon"; value: string }
  | { type: "dataUrl"; value: string };

export interface ScoredEntry {
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
// Channel Messages
// =========================================================

export type SearchMessage =
  | { type: "catalogResults"; entries: ScoredEntry[] }
  | { type: "done" };
