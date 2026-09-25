// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Last-resort error boundary at the root of a window.
 *
 * Gadget components have their own boundaries; this one catches errors
 * in the host UI around them. The launcher window is hidden and shown
 * rather than recreated, so without a reload a blank launcher would
 * stay blank until the app restarts.
 */

import { useCallback, type ReactNode } from "react";
import { createLogger } from "../lib/logger";
import { ErrorBoundary } from "./ErrorBoundary";

interface WindowErrorBoundaryProps {
  /** Window name, logged with the error. */
  window: string;
  children: ReactNode;
}

export function WindowErrorBoundary({ window: windowName, children }: WindowErrorBoundaryProps) {
  const logError = useCallback(
    (error: Error, componentStack: string) => {
      createLogger("host").error(`window failed to render: ${error.message}`, [
        ["window", windowName],
        ["stack", error.stack ?? ""],
        ["componentStack", componentStack],
      ]);
    },
    [windowName],
  );

  return (
    <ErrorBoundary
      onError={logError}
      fallback={(error) => (
        <div className="flex h-screen items-center justify-center p-4 font-sans antialiased">
          <div
            role="alert"
            className="flex max-w-md flex-col items-center gap-3 rounded-xl bg-surface p-6 text-center text-sm text-text-primary shadow-lg"
          >
            <span className="font-medium">Something went wrong in this window.</span>
            <span className="break-words font-mono text-xs text-text-muted">{error.message}</span>
            <button
              type="button"
              onClick={() => window.location.reload()}
              className="rounded-lg px-2.5 py-1 text-xs font-medium text-accent transition-colors hover:bg-accent/10"
            >
              Reload window
            </button>
          </div>
        </div>
      )}
    >
      {children}
    </ErrorBoundary>
  );
}
