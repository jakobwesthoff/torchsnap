// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import {
  lstatSync,
  lutimesSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  rmSync,
  statSync,
  symlinkSync,
  utimesSync,
  writeFileSync,
} from "fs";
import { tmpdir } from "os";
import { dirname, join } from "path";
import { build } from "vite";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { keepUnchangedMtimes, restoreUnchanged, snapshotTree } from "./keep-mtimes";

// Every timestamp is set explicitly, so no test waits for the clock.
const OLD = new Date("2001-01-01T00:00:00Z");
const NEW = new Date("2020-01-01T00:00:00Z");

let tmp: string;
let dist: string;

beforeEach(() => {
  tmp = mkdtempSync(join(tmpdir(), "keep-mtimes-"));
  dist = join(tmp, "dist");
});

afterEach(() => {
  rmSync(tmp, { recursive: true, force: true });
});

function writeTree(dir: string, files: Record<string, string>) {
  mkdirSync(dir, { recursive: true });
  for (const [path, content] of Object.entries(files)) {
    mkdirSync(dirname(join(dir, path)), { recursive: true });
    writeFileSync(join(dir, path), content);
  }
}

/** Sets `dir` and everything below it to `date`, symlinks themselves included. */
function setTimes(dir: string, date: Date) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) setTimes(path, date);
    else lutimesSync(path, date, date);
  }
  utimesSync(dir, date, date);
}

/** What Vite does to the output directory: empty it, write everything anew. */
function rebuild(dir: string, files: Record<string, string>) {
  for (const entry of readdirSync(dir)) rmSync(join(dir, entry), { recursive: true, force: true });
  writeTree(dir, files);
  setTimes(dir, NEW);
}

/** Records `files` in `dist` at OLD, then rebuilds it with `next`. */
function buildTwice(files: Record<string, string>, next: Record<string, string>) {
  writeTree(dist, files);
  setTimes(dist, OLD);
  const before = snapshotTree(dist);
  rebuild(dist, next);
  restoreUnchanged(dist, before);
}

const mtime = (path: string) => lstatSync(path).mtime;

describe("restoreUnchanged", () => {
  it("gives a byte-identical file its old timestamp back", () => {
    const files = { "index.html": "<html></html>", "assets/a.js": "export {}" };
    buildTwice(files, files);
    expect(mtime(join(dist, "index.html"))).toEqual(OLD);
    expect(mtime(join(dist, "assets/a.js"))).toEqual(OLD);
  });

  it("keeps the new timestamp of a file whose content changed at the same size", () => {
    buildTwice({ "a.js": "aaaa", "b.js": "b" }, { "a.js": "bbbb", "b.js": "b" });
    expect(mtime(join(dist, "a.js"))).toEqual(NEW);
    expect(mtime(join(dist, "b.js"))).toEqual(OLD);
  });

  it("keeps the new timestamp of a file whose size changed", () => {
    buildTwice({ "a.js": "a" }, { "a.js": "aa" });
    expect(mtime(join(dist, "a.js"))).toEqual(NEW);
  });

  // The app's dep-info lists the files of its last compile. A file that
  // is new to that list is invisible to cargo, so with any added or
  // removed file every timestamp stays new.
  it("restores nothing when a file was added", () => {
    buildTwice({ "a.js": "a", "b.js": "b" }, { "a.js": "a", "b.js": "b", "c.js": "c" });
    expect(mtime(join(dist, "a.js"))).toEqual(NEW);
    expect(mtime(join(dist, "b.js"))).toEqual(NEW);
  });

  it("restores nothing when a file was removed", () => {
    buildTwice({ "a.js": "a", "b.js": "b", "c.js": "c" }, { "a.js": "a", "b.js": "b" });
    expect(mtime(join(dist, "a.js"))).toEqual(NEW);
    expect(mtime(join(dist, "b.js"))).toEqual(NEW);
  });

  it("restores nothing when a file was renamed", () => {
    buildTwice({ "a.js": "a", "b.js": "b" }, { "a.js": "a", "c.js": "b" });
    expect(mtime(join(dist, "a.js"))).toEqual(NEW);
  });

  it("restores nothing when the directory did not exist before the build", () => {
    const before = snapshotTree(dist);
    mkdirSync(dist);
    rebuild(dist, { "a.js": "a" });
    restoreUnchanged(dist, before);
    expect(mtime(join(dist, "a.js"))).toEqual(NEW);
  });

  it("does nothing when the directory is gone after the build", () => {
    writeTree(dist, { "a.js": "a" });
    const before = snapshotTree(dist);
    rmSync(dist, { recursive: true });
    expect(() => restoreUnchanged(dist, before)).not.toThrow();
  });

  it("neither records nor touches a symlink", () => {
    writeTree(dist, { "a.js": "a" });
    symlinkSync("a.js", join(dist, "link.js"));
    setTimes(dist, OLD);
    const before = snapshotTree(dist);
    expect([...before.keys()]).toEqual(["a.js"]);
    rebuild(dist, { "a.js": "a" });
    symlinkSync("a.js", join(dist, "link.js"));
    lutimesSync(join(dist, "link.js"), NEW, NEW);
    restoreUnchanged(dist, before);
    expect(mtime(join(dist, "link.js"))).toEqual(NEW);
    expect(mtime(join(dist, "a.js"))).toEqual(OLD);
  });
});

describe("keepUnchangedMtimes", () => {
  const plugin = keepUnchangedMtimes();
  const hooks = plugin as unknown as {
    configResolved: (config: { root: string; build: { outDir: string } }) => void;
    buildStart: () => void;
    closeBundle: () => void;
  };

  it("only runs for production builds", () => {
    expect(plugin.apply).toBe("build");
  });

  it("restores the timestamps under the configured outDir", () => {
    const out = join(tmp, "out");
    writeTree(out, { "a.js": "a" });
    setTimes(out, OLD);
    hooks.configResolved({ root: tmp, build: { outDir: "out" } });
    hooks.buildStart();
    rebuild(out, { "a.js": "a" });
    hooks.closeBundle();
    expect(mtime(join(out, "a.js"))).toEqual(OLD);
  });
});

// A real `vite build` empties the output directory, copies `public/` into
// it and writes the bundle. These tests show that the plugin's hooks run
// before the first and after the last of those steps.
describe("keepUnchangedMtimes in a Vite build", () => {
  let project: string;

  beforeEach(() => {
    project = join(tmp, "project");
    writeTree(project, {
      "index.html": '<!doctype html><script type="module" src="/main.js"></script>',
      "main.js": 'console.log("hello");',
      "public/logo.txt": "logo",
      "public/img/a.txt": "a",
    });
    dist = join(project, "dist");
  });

  const viteBuild = () =>
    build({
      root: project,
      configFile: false,
      logLevel: "silent",
      plugins: [keepUnchangedMtimes()],
    });

  function allFiles(dir: string): string[] {
    return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
      const path = join(dir, entry.name);
      return entry.isDirectory() ? allFiles(path) : [path];
    });
  }

  it("keeps every timestamp when nothing changed", async () => {
    await viteBuild();
    setTimes(dist, OLD);
    await viteBuild();
    for (const path of allFiles(dist)) expect(mtime(path), path).toEqual(OLD);
  });

  it("gives a changed public file a new timestamp and keeps the others", async () => {
    await viteBuild();
    setTimes(dist, OLD);
    writeFileSync(join(project, "public/logo.txt"), "new logo");
    await viteBuild();
    expect(statSync(join(dist, "logo.txt")).mtimeMs).toBeGreaterThan(OLD.getTime());
    expect(mtime(join(dist, "img/a.txt"))).toEqual(OLD);
    expect(mtime(join(dist, "index.html"))).toEqual(OLD);
  });

  it("keeps every new timestamp when a public file was added back", async () => {
    rmSync(join(project, "public/img/a.txt"));
    await viteBuild();
    setTimes(dist, OLD);
    writeFileSync(join(project, "public/img/a.txt"), "a");
    await viteBuild();
    for (const path of allFiles(dist)) {
      expect(statSync(path).mtimeMs, path).toBeGreaterThan(OLD.getTime());
    }
  });
});
