// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Shapes of `Phase` in `src-tauri/src/updates/mod.rs`, serialized with
// a `phase` tag and camelCase fields.

/** Emitted by the backend with the new phase whenever it changes. */
export const UPDATE_PHASE_CHANGED = "update-phase-changed";

export type LocationProblem = "diskImage" | "translocated";

export interface ReleaseNotes {
  version: string;
  date: string | null;
  /** CHANGELOG Markdown from the update feed; untrusted. */
  notes: string;
}

export interface AvailableUpdate {
  installed: string;
  version: string;
  /** Every release between the installed and the offered one, newest first. */
  releases: ReleaseNotes[];
  pendingGadgetChanges: number;
  locationProblem: LocationProblem | null;
  downloadUrl: string;
}

export type UpdatePhase =
  | { phase: "idle" }
  | { phase: "checking" }
  | { phase: "upToDate"; installed: string }
  | ({ phase: "available" } & AvailableUpdate)
  | { phase: "downloading"; version: string; downloaded: number; total: number | null }
  | { phase: "installing"; version: string }
  | { phase: "failed"; message: string };
