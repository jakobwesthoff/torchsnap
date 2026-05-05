// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Plugin Color Palette
//
// 16 deterministic colors assigned by hashing the plugin ID.
// Shared between LogItemRow (source dot) and ConsoleToolbar
// (source filter chips).
// =========================================================

export const GADGET_COLORS = [
  "bg-orange-400",
  "bg-sky-400",
  "bg-emerald-400",
  "bg-violet-400",
  "bg-rose-400",
  "bg-teal-400",
  "bg-indigo-400",
  "bg-lime-400",
  "bg-amber-400",
  "bg-cyan-400",
  "bg-green-400",
  "bg-purple-400",
  "bg-pink-400",
  "bg-blue-400",
  "bg-yellow-400",
  "bg-fuchsia-400",
];

export function gadgetColorIndex(gadgetId: string): number {
  let hash = 0;
  for (let i = 0; i < gadgetId.length; i++) {
    hash = ((hash << 5) - hash + gadgetId.charCodeAt(i)) | 0;
  }
  return Math.abs(hash) % GADGET_COLORS.length;
}
