// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// @torchsnap/plugin-sdk — Public Type Exports
//
// Everything a plugin needs to type its components and
// interact with host-provided data structures.
//
// Usage:
//   import type { PluginViewProps } from "@torchsnap/plugin-sdk";
//   import type { ActionId, SourcedEntry } from "@torchsnap/plugin-sdk";
// =========================================================

export type {
  PluginViewProps,
  InlineViewProps,
  PluginSettingsProps,
} from "./plugin";

export type {
  ActionId,
  ActionKeybinding,
  Action,
  EntryIcon,
  SourcedEntry,
  FooterHint,
  FooterState,
} from "./data";

export type { Logger, LogLevel } from "./logger";

export type { UsePluginSetting } from "./settings";
