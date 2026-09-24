// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Permission groups
//
// The backend lists every grant separately. For people, the
// question is what a gadget touches, so grants are grouped:
// programs it runs, network it reaches, files it reads or opens,
// links it opens, the clipboard, and data that stays its own.
// A group counts as broad when any of its grants is.
//
// Manifests often list a path for every operating system (a token
// file on macOS, Linux and Windows). Paths that cannot apply to
// the current system are set aside, so the list shows what the
// gadget can actually reach here.
// =========================================================

import type { ArgvRule, PermissionChange, PermissionItem } from "./types";

export type Platform = "macos" | "linux" | "windows";

export type GroupId = "programs" | "network" | "files" | "links" | "clipboard" | "ownData";

export interface GroupEntry {
  text: string;
  detail?: string;
  change: PermissionChange;
  broad: boolean;
}

export interface PermissionGroup {
  id: GroupId;
  title: string;
  icon: string;
  broad: boolean;
  entries: GroupEntry[];
  /** Entries that only apply on another operating system. */
  otherSystems: GroupEntry[];
}

const GROUPS: { id: GroupId; title: string; icon: string }[] = [
  { id: "programs", title: "Runs programs", icon: "heroicons:command-line" },
  { id: "network", title: "Network", icon: "heroicons:globe-alt" },
  { id: "files", title: "Files", icon: "heroicons:document" },
  { id: "links", title: "Opens links", icon: "heroicons:link" },
  { id: "clipboard", title: "Clipboard", icon: "heroicons:clipboard" },
  { id: "ownData", title: "Own data", icon: "heroicons:circle-stack" },
];

export function groupPermissions(
  items: PermissionItem[],
  removed: PermissionItem[],
  platform: Platform,
): PermissionGroup[] {
  const groups = new Map<GroupId, PermissionGroup>(
    GROUPS.map((group) => [group.id, { ...group, broad: false, entries: [], otherSystems: [] }]),
  );

  for (const item of [...items, ...removed]) {
    const { group: id, entry, path } = describe(item);
    const group = groups.get(id);
    if (!group) {
      continue;
    }
    if (path !== undefined && isForOtherSystem(path, platform)) {
      group.otherSystems.push(entry);
    } else {
      group.entries.push(entry);
    }
    group.broad ||= entry.broad && item.change !== "removed";
  }

  return [...groups.values()].filter(
    (group) => group.entries.length > 0 || group.otherSystems.length > 0,
  );
}

/** Where one grant belongs and how it reads. */
function describe(item: PermissionItem): { group: GroupId; entry: GroupEntry; path?: string } {
  const base = { change: item.change, broad: item.severity === "warning" };
  const permission = item.permission;
  switch (permission.kind) {
    case "command":
      return {
        group: "programs",
        entry: {
          ...base,
          text: permission.binary,
          detail:
            permission.argv.length === 0
              ? "without arguments"
              : `with ${permission.argv.map(describeArgv).join(", ")}`,
        },
      };
    case "httpOrigin":
      return {
        group: "network",
        entry: { ...base, text: permission.origin === "*" ? "any website" : permission.origin },
      };
    case "openerOpenPath":
      return { group: "files", entry: { ...base, text: "open files and folders" } };
    case "openerRevealPath":
      return { group: "files", entry: { ...base, text: "show files in the file manager" } };
    case "filesystemRead":
      return {
        group: "files",
        entry: { ...base, text: `read ${shortenPath(permission.pattern)}` },
        path: permission.pattern,
      };
    case "openerScheme":
      return { group: "links", entry: { ...base, text: `${permission.scheme}: links` } };
    case "clipboard":
      return { group: "clipboard", entry: { ...base, text: "copy text to the clipboard" } };
    case "websiteMetadata":
      return { group: "network", entry: { ...base, text: "website titles and icons" } };
    case "settings":
      return { group: "ownData", entry: { ...base, text: "its own settings" } };
    case "frecency":
      return { group: "ownData", entry: { ...base, text: "which results you pick most often" } };
    case "sqlStorage":
      return { group: "ownData", entry: { ...base, text: "its own database" } };
    case "iconCache":
      return { group: "ownData", entry: { ...base, text: "the app icon cache" } };
    case "pathResolver":
      return { group: "ownData", entry: { ...base, text: "standard folder locations" } };
  }
}

const PATH_WORDS: [string, string][] = [
  ["${home}", "~"],
  ["${xdg-config}", "config folder"],
  ["${xdg-data}", "data folder"],
  ["${gadget-data}", "the gadget's data"],
  ["${gadget-archive}", "the gadget's package"],
];

/** Replace substitution variables with words a user knows. */
export function shortenPath(pattern: string): string {
  return PATH_WORDS.reduce((text, [variable, word]) => text.split(variable).join(word), pattern);
}

const MACOS_ONLY = ["/Library/", "/Applications/", "/System/", "${home}/Library/"];
const LINUX_ONLY = ["/var/lib/", "/etc/", "/proc/", "/sys/", "/run/", "/usr/share/"];

/** Whether a path pattern can only exist on another operating system. */
export function isForOtherSystem(pattern: string, platform: Platform): boolean {
  const windowsPath = /^[A-Za-z]:\\/.test(pattern) || pattern.includes("\\");
  if (windowsPath) {
    return platform !== "windows";
  }
  if (MACOS_ONLY.some((prefix) => pattern.startsWith(prefix))) {
    return platform !== "macos";
  }
  if (LINUX_ONLY.some((prefix) => pattern.startsWith(prefix))) {
    return platform !== "linux";
  }
  return false;
}

/** What a replacing version asks for that the installed one did not, and the reverse. */
export function changeSummary(
  items: PermissionItem[],
  removed: PermissionItem[],
): { adds: string[]; drops: string[] } {
  return {
    adds: items.filter((item) => item.change === "added").map((item) => describe(item).entry.text),
    drops: removed.map((item) => describe(item).entry.text),
  };
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
      return `a path inside ${shortenPath(rule.root)}`;
    case "anyString":
      return "any text";
    case "rest":
      return `any number of: ${describeArgv(rule.constraint)}`;
  }
}
