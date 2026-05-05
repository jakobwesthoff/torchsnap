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
  /** Display text truncated for the list view. */
  displayText: string;
  /** Highest-priority format (e.g. "image", "text", "files"). */
  primaryFormat: string;
}

/** How a format's content is delivered. */
export type FormatData =
  | { type: "string"; data: string }
  | { type: "json"; document: unknown }
  | { type: "asset"; path: string };

/** Full entry returned by `load_full_entry` for the detail view. */
export interface ClipboardHistoryEntry {
  id: string;
  capturedAt: string;
  /** Full display text (up to 128K characters). */
  displayText: string;
  /** Entry type derived from format priority (e.g. "text", "files", "image"). */
  primaryFormat: string;
  /** All stored formats keyed by name, each with typed content. */
  formats: Record<string, FormatData>;
}
