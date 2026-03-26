// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Global shortcut configuration — toggles between display mode
 * (styled keycaps + "Change" button) and inline recording mode
 * that live-previews the held key combination and commits on
 * keyup.
 */

import { useCallback, useEffect, useState } from "react";
import { cn } from "../lib/cn";
import { invoke } from "@tauri-apps/api/core";
import { formatModifier, formatKey, type ModifierKey } from "../keybindings";
import { KeyCap } from "../components/KeyCap";

interface ShortcutSectionProps {
  globalShortcut: string;
  setGlobalShortcut: (v: string) => Promise<void>;
}

// =========================================================
// Key Helpers
// =========================================================

const MODIFIER_CODES = new Set([
  "MetaLeft", "MetaRight",
  "ControlLeft", "ControlRight",
  "AltLeft", "AltRight",
  "ShiftLeft", "ShiftRight",
]);

/**
 * Build a Tauri accelerator string from a keyboard event.
 *
 * Uses `e.code` (physical key name) for the non-modifier key,
 * which matches the names Tauri's shortcut parser expects
 * (e.g. "Space", "KeyA", "ArrowUp").
 *
 * Single-letter codes like "KeyA" are shortened to "A" since
 * Tauri accepts both forms.
 */
function buildAccelerator(e: KeyboardEvent): string {
  const parts: string[] = [];
  if (e.metaKey || e.ctrlKey) {
    parts.push("CommandOrControl");
  }
  if (e.altKey) {
    parts.push("Alt");
  }
  if (e.shiftKey) {
    parts.push("Shift");
  }

  if (!MODIFIER_CODES.has(e.code)) {
    // Shorten "KeyX" → "X" and "DigitN" → "N" for readability.
    const key = e.code
      .replace(/^Key([A-Z])$/, "$1")
      .replace(/^Digit([0-9])$/, "$1");
    parts.push(key);
  }

  return parts.join("+");
}

/** Whether the event contains a non-modifier key (complete combo). */
function isComplete(e: KeyboardEvent): boolean {
  return !MODIFIER_CODES.has(e.code);
}

/**
 * Parse a Tauri accelerator string into display-formatted parts
 * using the app's platform-aware key formatters.
 */
function accelToDisplayParts(shortcut: string): string[] {
  const tokens = shortcut.split("+");
  const parts: string[] = [];

  for (const token of tokens) {
    if (token === "CommandOrControl" || token === "CmdOrCtrl") {
      parts.push(formatModifier("Meta" as ModifierKey));
    } else if (token === "Shift") {
      parts.push(formatModifier("Shift" as ModifierKey));
    } else if (token === "Alt") {
      parts.push(formatModifier("Alt" as ModifierKey));
    } else {
      parts.push(formatKey(token));
    }
  }

  return parts;
}

/** Render shortcut keys as individually styled keycaps. */
function ShortcutKeys({ shortcut }: { shortcut: string }) {
  const parts = accelToDisplayParts(shortcut);

  return (
    <span className="inline-flex items-center gap-0.5">
      {parts.map((part, i) => (
        <KeyCap key={i}>{part}</KeyCap>
      ))}
    </span>
  );
}

// =========================================================
// Component
// =========================================================

export function ShortcutSection({
  globalShortcut,
  setGlobalShortcut,
}: ShortcutSectionProps) {
  const [recording, setRecording] = useState(false);
  // Live preview of whatever is currently held (modifiers only or full combo)
  const [preview, setPreview] = useState<string | null>(null);
  // A complete combo (modifier + non-modifier key) ready to commit
  const [pending, setPending] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const startRecording = useCallback(() => {
    setRecording(true);
    setPending(null);
    setError(null);
  }, []);

  const cancelRecording = useCallback(() => {
    setRecording(false);
    setPending(null);
    setPreview(null);
  }, []);

  useEffect(() => {
    if (!recording) {
      return;
    }

    const onKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      // Escape aborts recording without changing the shortcut.
      if (e.code === "Escape") {
        cancelRecording();
        return;
      }

      const current = buildAccelerator(e);
      if (current) {
        setPreview(current);
      }

      // Remember the combo if it includes a non-modifier key
      if (isComplete(e) && current) {
        setPending(current);
      }
    };

    const onKeyUp = () => {
      // When keys are released, commit if we have a complete combo,
      // otherwise just clear the preview (bare modifier release).
      setRecording(false);
    };

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
    };
  }, [recording, cancelRecording]);

  // When recording ends, register the pending shortcut if we have one.
  useEffect(() => {
    if (recording) {
      return;
    }
    if (!pending) {
      setPreview(null);
      return;
    }

    const combo = pending;
    setPending(null);
    setPreview(null);

    (async () => {
      try {
        await invoke("update_global_shortcut", { shortcut: combo });
        setGlobalShortcut(combo);
        setError(null);
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      }
    })();
  }, [recording, pending, globalShortcut, setGlobalShortcut]);

  return (
    <div className="rounded-xl border border-border">
      <div
        className={cn(
          "flex items-center justify-between px-4 py-3 transition-colors",
          recording && "bg-accent/10",
        )}
      >
        <span className="text-sm text-text-primary">
          {recording ? "Press new shortcut…" : "Global Shortcut"}
        </span>

        <div className="flex items-center gap-2.5">
          {recording ? (
            <>
              {preview ? (
                <ShortcutKeys shortcut={preview} />
              ) : (
                <span className="text-xs text-text-muted italic">
                  waiting…
                </span>
              )}
              <button
                type="button"
                onClick={cancelRecording}
                className="flex h-5 w-5 items-center justify-center rounded-full text-text-muted transition-colors hover:bg-surface-inset hover:text-text-primary"
                aria-label="Cancel shortcut recording"
              >
                <svg viewBox="0 0 12 12" className="h-3 w-3" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
                  <path d="M3 3l6 6M9 3l-6 6" />
                </svg>
              </button>
            </>
          ) : (
            <>
              <ShortcutKeys shortcut={globalShortcut} />
              <button
                type="button"
                onClick={startRecording}
                className="rounded-lg px-2.5 py-1 text-xs font-medium text-accent transition-colors hover:bg-accent/10"
              >
                Change
              </button>
            </>
          )}
        </div>
      </div>

      {error && (
        <p className="px-4 pb-3 text-sm text-red-500">{error}</p>
      )}
    </div>
  );
}
