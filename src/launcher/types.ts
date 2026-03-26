// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import type { ReactNode } from "react";

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
  modifiers?: string[];
  key: string;
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
  | { type: "dataUrl"; value: string }
  | { type: "assetIcon"; value: string }
  | { type: "emoji"; value: string };

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
// Footer Types
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
// Channel Messages
// =========================================================

export type SearchMessage =
  | {
      type: "catalogResults";
      entries: ScoredEntry[];
      /** Plugin ID when the plugin requested custom UI (ADR 0013). */
      customPluginView: string | null;
      /** The prefix that triggered exclusive routing (e.g., ":"). */
      matchedPrefix: string | null;
    }
  | { type: "done" };
