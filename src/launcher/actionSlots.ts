// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Action Slots
//
// An entry's actions sit in fixed slots, and the slot alone
// decides the key that runs an action and its default label
// (ADR 55). The host sends each entry's filled slots as
// `{ slot, label }`; everything the user sees about a slot
// beyond the gadget's own label comes from this table.
// =========================================================

import type { KeyCombo } from "../keybindings";
import type { Action, ActionSlot, FooterState } from "../types";

/** The key that runs each slot's action. `Meta` matches Cmd on
 *  macOS and Ctrl elsewhere. */
export const SLOT_KEYS: Record<ActionSlot, KeyCombo> = {
  primary: { modifiers: [], key: "Enter" },
  secondary: { modifiers: ["Meta"], key: "Enter" },
  copy: { modifiers: ["Meta"], key: "c" },
  reveal: { modifiers: ["Meta", "Shift"], key: "r" },
  delete: { modifiers: ["Meta"], key: "Backspace" },
  openSettings: { modifiers: ["Meta"], key: "," },
};

/** Labels for slots whose action the gadget left unlabeled. The
 *  same English text on every platform. `primary` and `secondary`
 *  have none: their meaning differs per gadget. */
const SLOT_DEFAULT_LABELS: Partial<Record<ActionSlot, string>> = {
  copy: "Copy",
  reveal: "Reveal in Finder",
  delete: "Delete",
  openSettings: "Open settings",
};

/** What the launcher shows for an action. A gadget must label its
 *  `primary` and `secondary` actions; one that does not gets the
 *  entry's title. */
export function actionLabel(action: Action, entryTitle: string): string {
  return action.label ?? SLOT_DEFAULT_LABELS[action.slot] ?? entryTitle;
}

/** Whether `actions` fills `slot`. */
export function hasSlot(actions: Action[], slot: ActionSlot): boolean {
  return actions.some((action) => action.slot === slot);
}

/** The footer for a list entry: the primary action on Enter, every
 *  other filled slot as a hint, in the order the host sent them. */
export function actionsToFooterState(actions: Action[], entryTitle: string): FooterState {
  const primary = actions.find((action) => action.slot === "primary");
  return {
    primary: primary
      ? { combo: SLOT_KEYS.primary, label: actionLabel(primary, entryTitle) }
      : undefined,
    hints: actions
      .filter((action) => action.slot !== "primary")
      .map((action) => ({ combo: SLOT_KEYS[action.slot], label: actionLabel(action, entryTitle) })),
  };
}

/** The key bindings to register for a list entry's actions. Enter
 *  for the primary slot is registered once by the launcher itself,
 *  so it is not part of this list. */
export function slotBindings(actions: Action[]): { slot: ActionSlot; combo: KeyCombo }[] {
  return actions
    .filter((action) => action.slot !== "primary")
    .map((action) => ({ slot: action.slot, combo: SLOT_KEYS[action.slot] }));
}
