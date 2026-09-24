// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Reusable shortcut key combo recorder.
 *
 * Displays the current combo as styled keycaps with a "Change"
 * button. Clicking "Change" enters recording mode where the
 * component live-previews held keys and commits on keyup when
 * a complete combo (modifier + non-modifier) is detected.
 * Escape cancels recording.
 *
 * The component is backend-agnostic — it calls `onChange` with
 * the new Tauri accelerator string and leaves persistence to
 * the parent.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { ShortcutKeys } from "./ShortcutKeys";

// =========================================================
// Key Helpers
// =========================================================

const MODIFIER_CODES = new Set([
  "MetaLeft",
  "MetaRight",
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "ShiftLeft",
  "ShiftRight",
]);

/**
 * Build a Tauri accelerator string from a keyboard event.
 *
 * Uses `e.code` (physical key name) for the non-modifier key,
 * which matches the names Tauri's shortcut parser expects
 * (e.g. "Space", "KeyA", "ArrowUp").
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
    const key = e.code.replace(/^Key([A-Z])$/, "$1").replace(/^Digit([0-9])$/, "$1");
    parts.push(key);
  }

  return parts.join("+");
}

function isComplete(e: KeyboardEvent): boolean {
  return !MODIFIER_CODES.has(e.code);
}

// =========================================================
// Component
// =========================================================

interface ShortcutRecorderProps {
  /** Current Tauri accelerator string (e.g., "CmdOrCtrl+Shift+V"). */
  value: string;
  /** Called with the new accelerator string when a combo is recorded. */
  onChange: (combo: string) => void;
  disabled?: boolean;
}

export function ShortcutRecorder({ value, onChange, disabled = false }: ShortcutRecorderProps) {
  const [recording, setRecording] = useState(false);
  const [preview, setPreview] = useState<string | null>(null);

  // Mutable ref so the keyUp handler can read the latest pending combo
  // and onChange callback without re-subscribing event listeners.
  const pendingRef = useRef<string | null>(null);
  const onChangeRef = useRef(onChange);
  // ESLINT: This ref is only read from event handlers (onKeyUp), which
  // cannot fire during render. The theoretical concern with ref writes
  // during render is abandoned concurrent renders leaving stale values,
  // but that is harmless here — the next committed render overwrites it
  // immediately, and no event handler can observe the intermediate state.
  // Wrapping this in useEffect would actually be *less* correct: there
  // is a brief window between commit and effect execution where an event
  // could fire and read the previous onChange, missing a prop update.
  // Direct assignment during render guarantees the ref is current before
  // any post-render event can read it.
  // eslint-disable-next-line react-hooks/refs
  onChangeRef.current = onChange;

  const startRecording = useCallback(() => {
    if (disabled) return;
    setRecording(true);
    pendingRef.current = null;
  }, [disabled]);

  const cancelRecording = useCallback(() => {
    setRecording(false);
    pendingRef.current = null;
    setPreview(null);
  }, []);

  useEffect(() => {
    if (!recording) return;

    const onKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      if (e.code === "Escape") {
        cancelRecording();
        return;
      }

      const current = buildAccelerator(e);
      if (current) {
        setPreview(current);
      }

      if (isComplete(e) && current) {
        pendingRef.current = current;
      }
    };

    const onKeyUp = () => {
      const combo = pendingRef.current;
      pendingRef.current = null;
      setRecording(false);
      setPreview(null);
      if (combo) {
        onChangeRef.current(combo);
      }
    };

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
    };
  }, [recording, cancelRecording]);

  return (
    <div className="flex items-center gap-2.5">
      {recording ? (
        <>
          {preview ? (
            <ShortcutKeys shortcut={preview} />
          ) : (
            <span className="text-xs text-text-muted italic">waiting…</span>
          )}
          <button
            type="button"
            onMouseDown={(e) => {
              e.preventDefault();
              cancelRecording();
            }}
            className="flex h-5 w-5 items-center justify-center rounded-full text-text-muted transition-colors hover:bg-surface-inset hover:text-text-primary"
            aria-label="Cancel shortcut recording"
          >
            <svg
              viewBox="0 0 12 12"
              className="h-3 w-3"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
            >
              <path d="M3 3l6 6M9 3l-6 6" />
            </svg>
          </button>
        </>
      ) : (
        <>
          <ShortcutKeys shortcut={value} />
          <button
            type="button"
            onClick={startRecording}
            disabled={disabled}
            className={cn(
              "rounded-lg px-2.5 py-1 text-xs font-medium text-accent transition-colors hover:bg-accent/10",
              disabled && "opacity-40 cursor-not-allowed",
            )}
          >
            Change
          </button>
        </>
      )}
    </div>
  );
}
