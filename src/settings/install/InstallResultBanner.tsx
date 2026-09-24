// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Install results
//
// Everything installed in the current batch, with an Undo per
// entry and a single restart prompt, since nothing takes effect
// before the next start. Undo works until the banner is
// dismissed or the app restarts. Once every entry is undone and
// nothing else is pending for those gadgets, no restart is
// needed and the prompt goes away.
// =========================================================

import { relaunch } from "@tauri-apps/plugin-process";
import type { InstallResult } from "./useInstallQueue";

export function InstallResultBanner({
  results,
  onUndo,
  onDismiss,
}: {
  results: InstallResult[];
  onUndo: (gadgetId: string) => void;
  onDismiss: () => void;
}) {
  const needsRestart = results.some((result) => result.requiresRestart);
  return (
    <div className="flex flex-col gap-2 rounded-lg border border-accent/50 bg-accent/10 px-3 py-2 text-sm text-accent">
      <ul className="flex flex-col gap-1">
        {results.map((result) => (
          <li key={result.info.id} className="flex items-center justify-between gap-3">
            <span>{describeResult(result)}</span>
            {!result.undone && (
              <button
                type="button"
                onClick={() => onUndo(result.info.id)}
                className="shrink-0 rounded-md border border-accent/40 px-2 py-0.5 text-xs font-medium text-accent transition-colors hover:bg-accent/15"
              >
                Undo
              </button>
            )}
          </li>
        ))}
      </ul>
      <div className="flex items-center justify-end gap-2">
        {needsRestart ? (
          <>
            <span className="flex-1 text-xs">Changes take effect after a restart.</span>
            <button
              type="button"
              onClick={() => void relaunch()}
              className="rounded-md bg-accent px-2.5 py-1 text-xs font-medium text-white transition-colors hover:bg-accent/90"
            >
              Restart now
            </button>
          </>
        ) : (
          <span className="flex-1 text-xs">No restart needed.</span>
        )}
        <button
          type="button"
          onClick={onDismiss}
          title="Hide these results. The installs stay, but can no longer be undone."
          className="rounded-md px-1.5 py-0.5 text-xs hover:bg-surface-hover"
        >
          Dismiss
        </button>
      </div>
    </div>
  );
}

function describeResult({ info, undone }: InstallResult): string {
  const installed = info.previousVersion
    ? `Replaced ${info.name} ${info.previousVersion} with ${info.version}`
    : `Installed ${info.name} ${info.version}`;
  if (!undone) {
    return `${installed}.`;
  }
  return info.previousVersion
    ? `Undid replacing ${info.name}; ${info.previousVersion} stays.`
    : `Undid installing ${info.name} ${info.version}.`;
}
