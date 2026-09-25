// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Explains why a global shortcut does not work, below the row that
 * records it.
 *
 * The backend registers every shortcut on its own and keeps, per
 * settings key, the reason a shortcut is not registered. The note reads
 * those reasons on mount and again whenever the backend registers the
 * shortcuts anew (after any shortcut setting changes), and shows
 * nothing while the shortcut works.
 */

import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { command, type ShortcutProblem } from "../lib/command";

/** Emitted by the backend after it registered the shortcuts again. */
export const SHORTCUT_PROBLEMS_CHANGED = "shortcut-problems-changed";

function describe(problem: ShortcutProblem): string {
  switch (problem.kind) {
    case "missing":
      return "No shortcut is set. Record one to use it.";
    case "invalid":
      return `"${problem.combo}" is not a valid shortcut. Record a new one.`;
    case "takenBy":
      return `Already used by "${problem.label}". Record a different shortcut.`;
    case "rejected":
      return `The system did not accept this shortcut, so it does not work. Another app may already use it. (${problem.reason})`;
  }
}

function useShortcutProblem(settingsKey: string): ShortcutProblem | null {
  const [problem, setProblem] = useState<ShortcutProblem | null>(null);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    // Without an answer the note stays empty: a missing explanation is
    // better than a false one.
    const pull = () =>
      command("shortcut_problems").then(
        (problems) => {
          if (!cancelled) {
            setProblem(problems[settingsKey] ?? null);
          }
        },
        () => {},
      );

    void pull();
    listen(SHORTCUT_PROBLEMS_CHANGED, () => void pull()).then(
      (u) => {
        if (cancelled) {
          u();
        } else {
          unlisten = u;
        }
      },
      () => {},
    );
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [settingsKey]);

  return problem;
}

export function ShortcutProblemNote({ settingsKey }: { settingsKey: string }) {
  const problem = useShortcutProblem(settingsKey);
  if (!problem) {
    return null;
  }
  return (
    <p role="alert" className="text-xs text-red-500">
      {describe(problem)}
    </p>
  );
}
