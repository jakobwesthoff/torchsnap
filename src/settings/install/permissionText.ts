// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Permission wording
//
// The backend describes each grant structurally; this turns it
// into the sentence a user reads in the install review and on
// the gadget cards. Titles say what the gadget may do, details
// add the constraints that narrow it.
// =========================================================

import type { ArgvRule, Permission } from "./types";

export interface PermissionText {
  title: string;
  detail?: string;
}

export function permissionText(permission: Permission): PermissionText {
  switch (permission.kind) {
    case "command":
      return {
        title: `Run ${permission.binary}`,
        detail:
          permission.argv.length === 0
            ? "Without arguments"
            : `Arguments: ${permission.argv.map(describeArgv).join(", ")}`,
      };
    case "httpOrigin":
      return {
        title:
          permission.origin === "*" ? "Connect to any website" : `Connect to ${permission.origin}`,
      };
    case "openerOpenPath":
      return { title: "Open files and folders with their default app" };
    case "openerScheme":
      return { title: `Open ${permission.scheme}: links` };
    case "openerRevealPath":
      return { title: "Show files in the file manager" };
    case "filesystemRead":
      return { title: `Read files matching ${permission.pattern}` };
    case "clipboard":
      return { title: "Copy text to the clipboard" };
    case "websiteMetadata":
      return { title: "Look up website titles and icons" };
    case "settings":
      return { title: "Keep its own settings" };
    case "frecency":
      return { title: "See which results you pick most often" };
    case "sqlStorage":
      return { title: "Keep its own database" };
    case "iconCache":
      return { title: "Use the app icon cache" };
    case "pathResolver":
      return { title: "Look up standard folder locations" };
  }
}

/** One argument position of a command rule, in words. */
export function describeArgv(rule: ArgvRule): string {
  switch (rule.kind) {
    case "literal":
      return `"${rule.value}"`;
    case "enum":
      return `one of ${rule.values.map((value) => `"${value}"`).join(", ")}`;
    case "glob":
      return `text matching ${rule.pattern}`;
    case "regex":
      return `text matching /${rule.pattern}/`;
    case "pathUnder":
      return `a path inside ${rule.root}`;
    case "anyString":
      return "any text";
    case "rest":
      return `any number of: ${describeArgv(rule.constraint)}`;
  }
}
