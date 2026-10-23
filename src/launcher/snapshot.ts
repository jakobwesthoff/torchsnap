// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Builds the control API's `snapshot` answer from the launcher's
 * state. The Launcher hands its values in; the control channel hook
 * sends the result to the backend. Neither knows the other.
 */

import type { GadgetViewRef, LauncherSnapshot, SnapshotView, SourcedEntry } from "../types";

export interface SnapshotParts {
  /** Text in the search input. */
  displayQuery: string;
  /** The last search started has not finished. */
  searching: boolean;
  results: SourcedEntry[];
  /** The selected row of the result list (the selection minus the
   *  inline slot, when an inline view is shown). */
  listSelectedIndex: number;
  /** The inline view holds the selection. */
  inlineSelected: boolean;
  /** The gadget view that replaces the result list. */
  gadgetView: GadgetViewRef | null;
  /** The inline view above the result list. */
  inlineView: GadgetViewRef | null;
  contentVisible: boolean;
  inputFocused: boolean;
}

export function buildLauncherSnapshot(parts: SnapshotParts): LauncherSnapshot {
  // A gadget view replaces the result list; its results go to the view
  // and are not on screen.
  const results =
    parts.gadgetView === null
      ? parts.results.map(({ source, id, title, subtitle }) => ({ source, id, title, subtitle }))
      : [];
  // After an incremental merge the selection can point past the list
  // until the next render settles it.
  const rowSelected =
    !parts.inlineSelected &&
    parts.listSelectedIndex >= 0 &&
    parts.listSelectedIndex < results.length;

  return {
    query: parts.displayQuery,
    searching: parts.searching,
    results,
    selectedIndex: rowSelected ? parts.listSelectedIndex : null,
    inlineView: snapshotView(parts.inlineView),
    inlineViewSelected: parts.inlineSelected,
    gadgetView: snapshotView(parts.gadgetView),
    contentVisible: parts.contentVisible,
    inputFocused: parts.inputFocused,
  };
}

// A view's `data` is gadget-defined and can be large; the snapshot
// names the view only.
function snapshotView(view: GadgetViewRef | null): SnapshotView | null {
  return view && { gadgetId: view.gadgetId, view: view.view };
}
