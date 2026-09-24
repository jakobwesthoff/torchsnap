// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Typesafe wrapper around Tauri's `invoke()`.
 *
 * All frontend → backend calls go through `command()` instead of
 * raw `invoke()`. The command registry maps each Tauri command name
 * to its parameter and return types, enforced at compile time.
 *
 * Error handling: every rejection is logged via `console.warn` and
 * then re-thrown, so callers can add their own handling when needed
 * while unhandled failures are still visible during development.
 *
 * The `CommandMap` must stay in sync with the `generate_handler!`
 * registration and `#[tauri::command]` signatures in
 * `src-tauri/src/lib.rs` and `src-tauri/src/search/mod.rs`.
 *
 * TODO(structured-errors): see todos/backend/errors/01kn44hd21r2sv9y457dr9c11m-structured-rust-error-types.md
 */

import { invoke, type Channel } from "@tauri-apps/api/core";
import type { ActionId, ControlCommand, FrecencyStats, PostAction, SearchMessage } from "../types";
import type { DevToolsMessage, LogItem, LogLevel, LogStats } from "../devtools/types";
import type {
  InstallOrigin,
  InstallRequestView,
  PendingGadget,
  PermissionItem,
} from "../settings/install/types";

// =========================================================
// Command Registry
// =========================================================

export interface CommandMap {
  launcher_hide: { params: void; result: void };
  launcher_set_layout: {
    params: {
      windowWidth: number;
      windowHeight: number;
      cardTopOffset: number;
    };
    result: void;
  };
  search: {
    params: { query: string; onResults: Channel<SearchMessage> };
    result: void;
  };
  search_execute: {
    params: { source: string; entryId: string; actionId: ActionId };
    result: PostAction;
  };
  control_subscribe: {
    params: { channel: Channel<ControlCommand> };
    result: void;
  };
  gadget_message: {
    params: {
      source: string;
      method: string;
      payload: unknown;
      channel: Channel<unknown>;
    };
    result: unknown;
  };
  frecency_stats: { params: void; result: FrecencyStats };
  frecency_clear: { params: void; result: void };
  website_metadata_stats: { params: void; result: { entryCount: number; faviconBytes: number } };
  website_metadata_clear_cache: { params: void; result: void };
  devtools_log_history: {
    params: { afterSeq: number; limit: number };
    result: LogItem[];
  };
  devtools_log_subscribe: {
    params: { channel: Channel<DevToolsMessage> };
    result: void;
  };
  devtools_log_clear: { params: void; result: void };
  devtools_log_stats: { params: void; result: LogStats };
  logger_emit: {
    params: {
      source: string;
      level: LogLevel;
      message: string;
      metadata: [string, string][];
      spanId: number | null;
    };
    result: void;
  };
  logger_span_start: {
    params: {
      source: string;
      name: string;
      parentId: number | null;
      metadata: [string, string][];
    };
    result: number;
  };
  logger_span_end: {
    params: {
      spanId: number;
      metadata: [string, string][];
    };
    result: void;
  };
  wasm_gadgets: { params: void; result: WasmGadgetManifest[] };
  gadget_sources: { params: void; result: Record<string, GadgetSourceKind> };
  uninstall_user_gadget: {
    params: { gadgetId: string };
    result: UninstallResult;
  };
  install_undo: {
    params: { gadgetId: string };
    result: UndoResult;
  };
  install_queue_snapshot: { params: void; result: InstallRequestView[] };
  install_queue_submit: {
    params: { paths: string[]; origin: InstallOrigin };
    result: void;
  };
  install_queue_confirm: {
    params: { requestId: string };
    result: InstalledGadgetInfo;
  };
  install_queue_dismiss: { params: { requestId: string }; result: void };
  gadget_permissions: { params: void; result: Record<string, PermissionItem[]> };
  pending_gadget_changes: { params: void; result: Record<string, PendingGadget> };
  take_settings_start_section: { params: void; result: string | null };
  restart_to_apply_gadget_changes: { params: void; result: void };
  build_info: { params: void; result: { version: string; gitHash: string } };
}

// =========================================================
// Gadget Install / Uninstall
//
// Response shapes mirror `gadget_install.rs`. Serialized
// as JSON with camelCase field names so the TypeScript call
// sites stay idiomatic.
// =========================================================

/** Mirrors the Rust `VersionRelation` enum. */
export type VersionRelation = "upgrade" | "same" | "downgrade" | "unknown";

export interface InstalledGadgetInfo {
  id: string;
  name: string;
  version: string;
  /** Set when the install replaced another version of the gadget. */
  previousVersion: string | null;
  versionRelation: VersionRelation | null;
  requiresRestart: boolean;
}

export interface UndoResult {
  /** The version back on disk, or `null` when the undo removed the gadget. */
  restoredVersion: string | null;
  requiresRestart: boolean;
}

export interface UninstallResult {
  requiresRestart: boolean;
}

// =========================================================
// Gadget Source Kind
//
// Mirrors the Rust `GadgetSourceKind` enum. Serialized as
// lowercase strings across the Tauri IPC boundary. Used by
// the Gadgets settings panel to render source badges and
// gate the uninstall action to `user` gadgets.
// =========================================================

export type GadgetSourceKind = "builtin" | "system" | "user" | "dev";

// =========================================================
// WASM Gadget Manifest
//
// TypeScript mirror of the Rust `Manifest` struct. Sent from
// the backend as JSON with camelCase field names.
// =========================================================

export interface WasmGadgetManifest {
  gadget: {
    id: string;
    name: string;
    description: string;
    version: string;
    wasm: string;
    /** String icon identifier (e.g. "heroicons:hand-raised" or "icon.webp"). */
    icon: string;
    prefixes: string[];
  };
  settings: Record<string, unknown>;
  shortcuts: Record<string, { label: string; default: string }>;
  frontend?: {
    launcherBundle?: string;
    settingsBundle?: string;
    views: Record<string, string>;
    inlineViews: Record<string, string>;
    launcherCss?: string;
    settingsCss?: string;
    settings?: { component: string };
  };
}

export type CommandName = keyof CommandMap;

// =========================================================
// Typed command wrapper
// =========================================================

export async function command<C extends CommandName>(
  cmd: C,
  ...args: CommandMap[C]["params"] extends void ? [] : [CommandMap[C]["params"]]
): Promise<CommandMap[C]["result"]> {
  try {
    return await invoke<CommandMap[C]["result"]>(cmd, args[0] ?? undefined);
  } catch (error) {
    console.warn(`command("${cmd}") failed:`, error);
    throw error;
  }
}
