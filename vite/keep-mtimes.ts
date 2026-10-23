// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Keep the timestamps of unchanged build output
//
// Vite empties `dist/` and writes every file anew, so each build gives
// every file a new timestamp, even when its content is the same. The
// app embeds `dist/` through `tauri::generate_context!`, which lists
// every file it embeds in the app crate's dep-info. Cargo recompiles
// the crate when one of those files is newer than the last compile or
// missing, so each `vite build` made cargo embed the frontend again
// (ADR 68).
//
// This plugin records the files in `dist/` before the build and
// afterwards gives every byte-identical file its old timestamps back.
// A changed file keeps its new timestamp. A file that is new to the
// dep-info is invisible to cargo, though: nothing it lists changed. So
// the plugin restores timestamps only when the build produced exactly
// the files it found before. With any file added, removed or renamed it
// restores nothing, every file is newer than the last compile, and
// cargo embeds the frontend again. That also holds when the compile
// after such a build fails or never runs: the next build restores those
// timestamps, which are still newer than the last compile.
// =========================================================

import { createHash } from "crypto";
import { existsSync, readdirSync, readFileSync, statSync, utimesSync } from "fs";
import { join, resolve } from "path";
import type { Plugin } from "vite";

export interface FileRecord {
  size: number;
  sha256: string;
  atimeMs: number;
  mtimeMs: number;
}

/** Regular files by path relative to the root. */
export type TreeSnapshot = Map<string, FileRecord>;

const sha256 = (path: string) => createHash("sha256").update(readFileSync(path)).digest("hex");

/**
 * Paths of the regular files below `root`, relative to it. Symlinks and
 * other entry types are skipped: Vite writes none into `dist/`.
 */
function listFiles(root: string, relative = ""): string[] {
  return readdirSync(join(root, relative), { withFileTypes: true }).flatMap((entry) => {
    const child = relative === "" ? entry.name : `${relative}/${entry.name}`;
    if (entry.isDirectory()) return listFiles(root, child);
    return entry.isFile() ? [child] : [];
  });
}

export function snapshotTree(root: string): TreeSnapshot {
  const snapshot: TreeSnapshot = new Map();
  if (!existsSync(root)) return snapshot;
  for (const relative of listFiles(root)) {
    const path = join(root, relative);
    const { size, atimeMs, mtimeMs } = statSync(path);
    snapshot.set(relative, { size, sha256: sha256(path), atimeMs, mtimeMs });
  }
  return snapshot;
}

export function restoreUnchanged(root: string, before: TreeSnapshot) {
  if (!existsSync(root)) return;
  const files = listFiles(root);
  if (files.length !== before.size || !files.every((relative) => before.has(relative))) return;
  for (const relative of files) {
    const path = join(root, relative);
    const old = before.get(relative)!;
    if (old.size !== statSync(path).size || old.sha256 !== sha256(path)) continue;
    // `utimesSync` takes seconds as a double, so a restored time can be
    // off by well under a microsecond. Cargo compares against its last
    // compile, seconds later, so that does not matter.
    utimesSync(path, old.atimeMs / 1000, old.mtimeMs / 1000);
  }
}

/**
 * Vite plugin: give unchanged files in the output directory their
 * timestamps from before the build back.
 *
 * `buildStart` runs before Vite empties the output directory and copies
 * `public/` into it (both in `renderStart`), `closeBundle` after the
 * bundle is written, also when the build failed.
 */
export function keepUnchangedMtimes(): Plugin {
  let outDir = "";
  let before: TreeSnapshot | undefined;
  return {
    name: "torchsnap-keep-unchanged-mtimes",
    apply: "build",
    configResolved(config) {
      outDir = resolve(config.root, config.build.outDir);
    },
    buildStart() {
      before = snapshotTree(outDir);
    },
    closeBundle() {
      if (before) restoreUnchanged(outDir, before);
      before = undefined;
    },
  };
}
