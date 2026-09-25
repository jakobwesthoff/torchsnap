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

/** The slot an action sits in. The slot alone decides the key that
 *  runs the action and its default label (see
 *  `src/launcher/actionSlots.ts`). */
export type ActionSlot = "primary" | "secondary" | "copy" | "reveal" | "delete" | "openSettings";

/** One action of an entry. Its command stays in the host; the
 *  frontend runs it by naming the entry and the slot. */
export interface Action {
  slot: ActionSlot;
  /** `null` shows the slot's default label. */
  label: string | null;
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
