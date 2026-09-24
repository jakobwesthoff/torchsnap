// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it } from "vitest";
import {
  changeSummary,
  describeArgv,
  groupPermissions,
  isForOtherSystem,
  shortenPath,
} from "./permissionModel";
import type { Permission, PermissionChange, PermissionItem } from "./types";

function item(
  permission: Permission,
  severity: PermissionItem["severity"] = "notice",
  change: PermissionChange = "unchanged",
): PermissionItem {
  return { permission, severity, change };
}

const zerotierLike: PermissionItem[] = [
  item({ kind: "httpOrigin", origin: "http://localhost:9993" }),
  item({ kind: "filesystemRead", pattern: "${xdg-config}/ZeroTier/One/authtoken.secret" }),
  item({
    kind: "filesystemRead",
    pattern: "/Library/Application Support/ZeroTier/One/authtoken.secret",
  }),
  item({ kind: "filesystemRead", pattern: "/var/lib/zerotier-one/authtoken.secret" }),
  item({ kind: "filesystemRead", pattern: "C:\\ProgramData\\ZeroTier\\One\\authtoken.secret" }),
  item({ kind: "clipboard" }),
  item({ kind: "settings" }, "info"),
  item({ kind: "sqlStorage" }, "info"),
];

describe("groupPermissions", () => {
  it("groups permissions by what they touch, in a fixed order", () => {
    const groups = groupPermissions(
      [
        item({ kind: "settings" }, "info"),
        item({ kind: "openerScheme", scheme: "https" }),
        item({ kind: "clipboard" }),
        item({ kind: "command", binary: "/usr/bin/mdfind", argv: [] }, "warning"),
        item({ kind: "filesystemRead", pattern: "/tmp/*" }),
        item({ kind: "httpOrigin", origin: "https://api.example.com" }),
      ],
      [],
      "macos",
    );

    expect(groups.map((group) => group.title)).toEqual([
      "Runs programs",
      "Network",
      "Files",
      "Opens links",
      "Clipboard",
      "Own data",
    ]);
  });

  it("describes each entry in plain words", () => {
    const groups = groupPermissions(
      [
        item(
          {
            kind: "command",
            binary: "/usr/bin/mdfind",
            argv: [{ kind: "literal", value: "-name" }, { kind: "anyString" }],
          },
          "warning",
        ),
        item({ kind: "httpOrigin", origin: "*" }, "warning"),
        item({ kind: "openerOpenPath" }, "warning"),
        item({ kind: "openerRevealPath" }),
        item({ kind: "filesystemRead", pattern: "${home}/.config/weather/*" }),
        item({ kind: "openerScheme", scheme: "mailto" }),
        item({ kind: "clipboard" }),
        item({ kind: "frecency" }, "info"),
      ],
      [],
      "macos",
    );
    const entries = Object.fromEntries(
      groups.map((group) => [group.id, group.entries.map((e) => [e.text, e.detail])]),
    );

    expect(entries).toEqual({
      programs: [["/usr/bin/mdfind", 'with "-name", any text']],
      network: [["any website", undefined]],
      files: [
        ["open files and folders", undefined],
        ["show files in the file manager", undefined],
        ["read ~/.config/weather/*", undefined],
      ],
      links: [["mailto: links", undefined]],
      clipboard: [["copy text to the clipboard", undefined]],
      ownData: [["which results you pick most often", undefined]],
    });
  });

  it("marks a group with any broad entry as broad", () => {
    const groups = groupPermissions(
      [item({ kind: "httpOrigin", origin: "*" }, "warning"), item({ kind: "clipboard" })],
      [],
      "macos",
    );

    expect(groups.find((g) => g.id === "network")?.broad).toBe(true);
    expect(groups.find((g) => g.id === "clipboard")?.broad).toBe(false);
  });

  it("puts paths for other operating systems aside", () => {
    const files = groupPermissions(zerotierLike, [], "macos").find((g) => g.id === "files");

    expect(files?.entries.map((e) => e.text)).toEqual([
      "read config folder/ZeroTier/One/authtoken.secret",
      "read /Library/Application Support/ZeroTier/One/authtoken.secret",
    ]);
    expect(files?.otherSystems.map((e) => e.text)).toEqual([
      "read /var/lib/zerotier-one/authtoken.secret",
      "read C:\\ProgramData\\ZeroTier\\One\\authtoken.secret",
    ]);
  });

  it("keeps changes and adds removed permissions to their groups", () => {
    const groups = groupPermissions(
      [item({ kind: "httpOrigin", origin: "*" }, "warning", "added")],
      [item({ kind: "clipboard" }, "notice", "removed")],
      "macos",
    );

    expect(groups.find((g) => g.id === "network")?.entries[0].change).toBe("added");
    expect(groups.find((g) => g.id === "clipboard")?.entries[0].change).toBe("removed");
  });
});

describe("shortenPath", () => {
  it("replaces substitution variables with words a user knows", () => {
    expect(shortenPath("${home}/Documents/*")).toBe("~/Documents/*");
    expect(shortenPath("${xdg-config}/app/config.toml")).toBe("config folder/app/config.toml");
    expect(shortenPath("${xdg-data}/app")).toBe("data folder/app");
    expect(shortenPath("${gadget-data}/cache")).toBe("the gadget's data/cache");
    expect(shortenPath("/tmp/*")).toBe("/tmp/*");
  });
});

describe("isForOtherSystem", () => {
  it("recognizes Windows paths everywhere but on Windows", () => {
    expect(isForOtherSystem("C:\\ProgramData\\x", "macos")).toBe(true);
    expect(isForOtherSystem("C:\\ProgramData\\x", "linux")).toBe(true);
    expect(isForOtherSystem("C:\\ProgramData\\x", "windows")).toBe(false);
  });

  it("recognizes macOS-only and Linux-only locations", () => {
    expect(isForOtherSystem("/Library/Application Support/x", "linux")).toBe(true);
    expect(isForOtherSystem("/Library/Application Support/x", "macos")).toBe(false);
    expect(isForOtherSystem("/var/lib/x", "macos")).toBe(true);
    expect(isForOtherSystem("/etc/x", "macos")).toBe(true);
    expect(isForOtherSystem("/var/lib/x", "linux")).toBe(false);
  });

  it("treats paths that work everywhere as current", () => {
    expect(isForOtherSystem("${home}/.config/x", "macos")).toBe(false);
    expect(isForOtherSystem("/tmp/x", "linux")).toBe(false);
  });
});

describe("changeSummary", () => {
  it("lists what a new version adds and drops", () => {
    expect(
      changeSummary(
        [
          item({ kind: "httpOrigin", origin: "*" }, "warning", "added"),
          item({ kind: "settings" }, "info", "unchanged"),
        ],
        [item({ kind: "clipboard" }, "notice", "removed")],
      ),
    ).toEqual({ adds: ["any website"], drops: ["copy text to the clipboard"] });
  });
});

describe("describeArgv", () => {
  it("describes every argument constraint", () => {
    expect(describeArgv({ kind: "literal", value: "status" })).toBe('"status"');
    expect(describeArgv({ kind: "enum", values: ["log", "diff"] })).toBe('one of "log", "diff"');
    expect(describeArgv({ kind: "glob", pattern: "*.md" })).toBe("text matching *.md");
    expect(describeArgv({ kind: "regex", pattern: "^[0-9]+$" })).toBe("text matching /^[0-9]+$/");
    expect(describeArgv({ kind: "pathUnder", root: "${home}" })).toBe("a path inside ~");
    expect(describeArgv({ kind: "anyString" })).toBe("any text");
    expect(describeArgv({ kind: "rest", constraint: { kind: "anyString" } })).toBe(
      "any number of: any text",
    );
  });
});
