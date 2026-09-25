// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Platform detection and OS-aware formatting for keyboard shortcuts.
 *
 * Provides display strings for modifier keys and special keys that match
 * platform conventions (e.g. ⌘ on macOS vs Ctrl on Windows/Linux).
 */

import type { ModifierKey } from "./matching";

// =========================================================
// Platform Detection
// =========================================================

export type Platform = "macos" | "windows" | "linux";

/**
 * Detect the current platform at module load time.
 *
 * Uses the modern `navigator.userAgentData.platform` API when available,
 * falling back to the legacy `navigator.platform` string. Anything that
 * is neither macOS nor Windows counts as Linux.
 */
export function detectPlatform(): Platform {
  const ua = navigator as {
    userAgentData?: { platform?: string };
    platform?: string;
  };
  const raw: string = ua.userAgentData?.platform ?? ua.platform ?? "";
  if (/mac/i.test(raw)) {
    return "macos";
  }
  return /win/i.test(raw) ? "windows" : "linux";
}

export const platform: Platform = detectPlatform();

// =========================================================
// Modifier Formatting
//
// Shortcuts are shown with the physical modifier keys they need.
// Keybindings declare `Meta`, which is Cmd on macOS and Ctrl
// elsewhere; global shortcuts name the keys directly. Both are
// resolved to physical keys, then shown in one order on every
// platform: Ctrl, Alt, Shift, Cmd/Win/Super. On macOS that is the
// order menus use (⌃⌥⇧⌘).
// =========================================================

/** A physical modifier key, named as Tauri's accelerator parser names it. */
export type PhysicalModifier = "Control" | "Alt" | "Shift" | "Super";

const MODIFIER_ORDER: readonly PhysicalModifier[] = ["Control", "Alt", "Shift", "Super"];

const MODIFIER_LABELS: Record<Platform, Record<PhysicalModifier, string>> = {
  macos: { Control: "⌃", Alt: "⌥", Shift: "⇧", Super: "⌘" },
  windows: { Control: "Ctrl", Alt: "Alt", Shift: "Shift", Super: "Win" },
  linux: { Control: "Ctrl", Alt: "Alt", Shift: "Shift", Super: "Super" },
};

/** The physical key a keybinding modifier stands for on a given platform. */
export function physicalModifierFor(modifier: ModifierKey, p: Platform): PhysicalModifier {
  switch (modifier) {
    case "Meta":
      return p === "macos" ? "Super" : "Control";
    case "Ctrl":
      return "Control";
    case "Alt":
      return "Alt";
    case "Shift":
      return "Shift";
  }
}

/**
 * Display labels for a set of modifiers, in the shared order and
 * without duplicates.
 *
 * Exported with explicit platform parameter for testability;
 * consumers typically use `formatModifiers` which reads the detected platform.
 */
export function formatModifiersForPlatform(
  modifiers: readonly PhysicalModifier[],
  p: Platform,
): string[] {
  return MODIFIER_ORDER.filter((m) => modifiers.includes(m)).map((m) => MODIFIER_LABELS[p][m]);
}

export function formatModifiers(modifiers: readonly PhysicalModifier[]): string[] {
  return formatModifiersForPlatform(modifiers, platform);
}

// =========================================================
// Key Formatting
// =========================================================

/** Keys with the same display string on all platforms. */
const UNIVERSAL_KEY_MAP: Record<string, string> = {
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Enter: "↵",
  Escape: "Esc",
  Space: "Space",
  " ": "Space",
  Tab: "⇥",
};

/** Keys with platform-specific display strings. */
const PLATFORM_KEY_MAP: Record<string, Record<Platform, string>> = {
  Backspace: { macos: "⌫", windows: "Backspace", linux: "Backspace" },
  Delete: { macos: "⌦", windows: "Del", linux: "Del" },
};

/**
 * Format a key name for display on a given platform.
 *
 * Single lowercase letters are uppercased. Special keys (arrows, Enter, etc.)
 * are mapped to symbols. Everything else is passed through as-is.
 */
export function formatKeyForPlatform(key: string, p: Platform): string {
  if (key in UNIVERSAL_KEY_MAP) {
    return UNIVERSAL_KEY_MAP[key];
  }
  if (key in PLATFORM_KEY_MAP) {
    return PLATFORM_KEY_MAP[key][p];
  }

  // Single lowercase letter → uppercase for display
  if (key.length === 1 && key >= "a" && key <= "z") {
    return key.toUpperCase();
  }

  return key;
}

export function formatKey(key: string): string {
  return formatKeyForPlatform(key, platform);
}
