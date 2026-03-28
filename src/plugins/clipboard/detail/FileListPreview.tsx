// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import type { RefObject } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { DocumentDuplicateIcon } from "@heroicons/react/24/outline";
import type { ClipboardHistoryEntry } from "../types";

/**
 * Extracts the filename from a full path by splitting on the last
 * path separator. Returns the full string if no separator is found.
 */
function filename(path: string): string {
  const lastSep = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return lastSep === -1 ? path : path.slice(lastSep + 1);
}

/**
 * Extracts the parent directory from a full path. Returns an empty
 * string if there is no parent (i.e. the path is a bare filename).
 */
function parentDir(path: string): string {
  const lastSep = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return lastSep === -1 ? "" : path.slice(0, lastSep);
}

/**
 * Detail preview for file-list clipboard entries.
 *
 * Layout:
 * - Top: Finder icon image (single-file copies where macOS provides an
 *   image format alongside the file list) or a `DocumentDuplicateIcon`
 *   placeholder for multi-file copies.
 * - Bottom: file count header, followed by each file's name and parent
 *   directory path.
 */
export function FileListPreview({
  detail,
  scrollRef,
}: {
  detail: ClipboardHistoryEntry;
  scrollRef: RefObject<HTMLDivElement | null>;
}) {
  const filesFormat = detail.formats.files;
  const paths: string[] =
    filesFormat?.type === "json" && Array.isArray(filesFormat.document)
      ? (filesFormat.document as string[])
      : [];

  const imageFormat = detail.formats.image;
  const hasFinderIcon = imageFormat?.type === "asset";

  return (
    <div
      ref={scrollRef}
      className="w-[60%] overflow-y-auto overflow-x-hidden p-4 scrollbar-accent flex flex-col items-center justify-center gap-4"
    >
      {/* ------------------------------------------------- */}
      {/* Icon area — Finder icon or multi-file placeholder  */}
      {/* ------------------------------------------------- */}
      <div className="flex justify-center">
        {hasFinderIcon ? (
          <img
            src={convertFileSrc(imageFormat.path)}
            alt="File icon"
            className="max-w-[128px] max-h-[128px] object-contain"
            draggable={false}
          />
        ) : (
          <DocumentDuplicateIcon className="h-16 w-16 text-text-muted" />
        )}
      </div>

      {/* ------------------------------------------------- */}
      {/* File list                                          */}
      {/* ------------------------------------------------- */}
      <div className="flex flex-col gap-1 max-w-full min-w-0">
        <span className="text-xs text-text-muted select-none">
          {paths.length} {paths.length === 1 ? "file" : "files"}
        </span>
        {paths.map((path) => (
          <div key={path} className="flex flex-col py-1">
            <span className="text-sm text-text-primary truncate">
              {filename(path)}
            </span>
            {parentDir(path) && (
              <span className="text-xs text-text-muted truncate">
                {parentDir(path)}
              </span>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
