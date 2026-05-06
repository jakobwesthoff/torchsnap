// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Shared Data Types
//
// Gadget-facing subset of the data types shared between the
// host and gadget frontends. These mirror the Rust-side
// types in src-tauri/src/search/types.rs.
//
// Only types that gadgets need to consume or produce are
// included here. Host-only types (SearchMessage,
// ControlCommand, etc.) are intentionally excluded.
// =========================================================

import type { ReactNode } from "react";

// ---- Actions --------------------------------------------

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

// ---- Entries --------------------------------------------

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

// ---- Footer ---------------------------------------------

export interface FooterHint {
  combo?: { modifiers: string[]; key: string };
  label: ReactNode;
}

export interface FooterState {
  primary?: FooterHint;
  hints: FooterHint[];
}
