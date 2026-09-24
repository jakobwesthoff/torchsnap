// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it } from "vitest";
import { describeArgv, permissionText } from "./permissionText";

describe("permissionText", () => {
  it("describes a command with its arguments", () => {
    expect(
      permissionText({
        kind: "command",
        binary: "/usr/bin/mdfind",
        argv: [{ kind: "literal", value: "-name" }, { kind: "anyString" }],
      }),
    ).toEqual({
      title: "Run /usr/bin/mdfind",
      detail: 'Arguments: "-name", any text',
    });
  });

  it("describes a command without arguments", () => {
    expect(permissionText({ kind: "command", binary: "uptime", argv: [] })).toEqual({
      title: "Run uptime",
      detail: "Without arguments",
    });
  });

  it("describes every argument constraint", () => {
    expect(describeArgv({ kind: "literal", value: "status" })).toBe('"status"');
    expect(describeArgv({ kind: "enum", values: ["log", "diff"] })).toBe('one of "log", "diff"');
    expect(describeArgv({ kind: "glob", pattern: "*.md" })).toBe("text matching *.md");
    expect(describeArgv({ kind: "regex", pattern: "^[0-9]+$" })).toBe("text matching /^[0-9]+$/");
    expect(describeArgv({ kind: "pathUnder", root: "${home}" })).toBe("a path inside ${home}");
    expect(describeArgv({ kind: "anyString" })).toBe("any text");
    expect(describeArgv({ kind: "rest", constraint: { kind: "anyString" } })).toBe(
      "any number of: any text",
    );
  });

  it("describes network access, calling out any website", () => {
    expect(permissionText({ kind: "httpOrigin", origin: "https://api.example.com" })).toEqual({
      title: "Connect to https://api.example.com",
    });
    expect(permissionText({ kind: "httpOrigin", origin: "*" })).toEqual({
      title: "Connect to any website",
    });
  });

  it("describes opener permissions", () => {
    expect(permissionText({ kind: "openerOpenPath" }).title).toBe(
      "Open files and folders with their default app",
    );
    expect(permissionText({ kind: "openerScheme", scheme: "mailto" }).title).toBe(
      "Open mailto: links",
    );
    expect(permissionText({ kind: "openerRevealPath" }).title).toBe(
      "Show files in the file manager",
    );
  });

  it("describes filesystem reads", () => {
    expect(
      permissionText({ kind: "filesystemRead", pattern: "${home}/.config/weather/*" }),
    ).toEqual({ title: "Read files matching ${home}/.config/weather/*" });
  });

  it("describes every flag permission", () => {
    const titles = (
      [
        "clipboard",
        "websiteMetadata",
        "settings",
        "frecency",
        "sqlStorage",
        "iconCache",
        "pathResolver",
      ] as const
    ).map((kind) => permissionText({ kind }).title);

    expect(titles).toEqual([
      "Copy text to the clipboard",
      "Look up website titles and icons",
      "Keep its own settings",
      "See which results you pick most often",
      "Keep its own database",
      "Use the app icon cache",
      "Look up standard folder locations",
    ]);
  });
});
