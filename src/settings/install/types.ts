// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Install queue contract
//
// Mirrors of the Rust types in `src-tauri/src/gadget_install/`
// (`queue.rs`, `review.rs`, `provenance.rs`, `intake.rs`). The
// serialization test `a_review_serializes_to_the_frontend_contract`
// in `review.rs` pins the JSON shape these describe.
// =========================================================

import type { VersionRelation } from "../../lib/command";

export type InstallOrigin = "settingsPicker" | "settingsDrop" | "osOpenFile" | "commandLine";

export type ArgvRule =
  | { kind: "literal"; value: string }
  | { kind: "enum"; values: string[] }
  | { kind: "glob"; pattern: string }
  | { kind: "regex"; pattern: string }
  | { kind: "pathUnder"; root: string }
  | { kind: "anyString" }
  | { kind: "rest"; constraint: ArgvRule };

export type Permission =
  | { kind: "command"; binary: string; argv: ArgvRule[] }
  | { kind: "httpOrigin"; origin: string }
  | { kind: "openerOpenPath" }
  | { kind: "openerScheme"; scheme: string }
  | { kind: "openerRevealPath" }
  | { kind: "filesystemRead"; pattern: string }
  | { kind: "clipboard" }
  | { kind: "websiteMetadata" }
  | { kind: "settings" }
  | { kind: "frecency" }
  | { kind: "sqlStorage" }
  | { kind: "iconCache" }
  | { kind: "pathResolver" };

export type Severity = "info" | "notice" | "warning";

export type PermissionChange = "added" | "unchanged" | "removed";

export interface PermissionItem {
  permission: Permission;
  severity: Severity;
  change: PermissionChange;
}

export interface Provenance {
  downloadUrl: string | null;
  referrerUrl: string | null;
  downloadedBy: string | null;
}

export type ReviewAction =
  | { kind: "install" }
  | { kind: "replace"; previousVersion: string; relation: VersionRelation }
  | { kind: "reject"; reason: string };

export interface InstallReview {
  gadget: { id: string; name: string; description: string; version: string };
  sourcePath: string;
  provenance: Provenance | null;
  action: ReviewAction;
  permissions: PermissionItem[];
  removedPermissions: PermissionItem[];
}

export type RequestState =
  | { kind: "staging" }
  | { kind: "ready"; review: InstallReview }
  | { kind: "failed"; message: string };

export interface InstallRequestView {
  id: string;
  origin: InstallOrigin;
  sourcePath: string;
  state: RequestState;
}

/** Event the backend emits after every change to the install queue. */
export const INSTALL_QUEUE_CHANGED = "install-queue-changed";
