// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Types for the clipboard manager plugin frontend.
 *
 * These mirror the Rust-side serialized types from
 * `plugins::clipboard::schema`.
 */

/** Lightweight entry used for list display — sent via subscribe/notify. */
export interface ClipboardListEntry {
  id: string;
  capturedAt: string;
  preview: string;
  /** Highest-priority format (e.g. "image", "text", "files"). */
  primaryFormat: string;
}

/** Full entry returned by `load_full_entry` for the detail preview. */
export interface ClipboardHistoryEntry {
  id: string;
  capturedAt: string;
  preview: string;
  formats: string[];
  /** Absolute path to the image file, if this entry has an image. */
  imagePath: string | null;
}
