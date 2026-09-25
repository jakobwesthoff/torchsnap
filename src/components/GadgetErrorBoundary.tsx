// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Error boundary around one gadget's frontend component.
 *
 * Gadget views and settings panels are third-party code loaded at
 * runtime. A render error in one of them shows an error card in its
 * place, and the rest of the window keeps working. The error is logged
 * under the gadget's id, so the devtools console attributes it to the
 * gadget.
 */

import { useCallback, type ReactNode } from "react";
import { createLogger } from "../lib/logger";
import { ErrorBoundary } from "./ErrorBoundary";
import { Icon } from "./Icon";

interface GadgetErrorBoundaryProps {
  gadgetId: string;
  children: ReactNode;
  /** Leaves the broken view. Without it the card offers no way out. */
  onGoBack?: () => void;
}

export function GadgetErrorBoundary({ gadgetId, children, onGoBack }: GadgetErrorBoundaryProps) {
  const logError = useCallback(
    (error: Error, componentStack: string) => {
      createLogger(gadgetId).error(`view failed to render: ${error.message}`, [
        ["stack", error.stack ?? ""],
        ["componentStack", componentStack],
      ]);
    },
    [gadgetId],
  );

  return (
    <ErrorBoundary
      onError={logError}
      fallback={(error) => (
        <div
          role="alert"
          className="m-3 flex items-start gap-3 rounded-lg bg-surface-group p-3 text-sm"
        >
          <Icon
            icon="heroicons:exclamation-triangle"
            className="mt-0.5 h-5 w-5 shrink-0 text-accent"
          />
          <div className="flex min-w-0 flex-1 flex-col gap-1">
            <span className="text-text-primary">
              The <span className="font-medium">{gadgetId}</span> gadget could not show this view.
            </span>
            <span className="break-words font-mono text-xs text-text-muted">{error.message}</span>
          </div>
          {onGoBack && (
            <button
              type="button"
              onClick={onGoBack}
              className="shrink-0 rounded-lg px-2.5 py-1 text-xs font-medium text-accent transition-colors hover:bg-accent/10"
            >
              Go back
            </button>
          )}
        </div>
      )}
    >
      {children}
    </ErrorBoundary>
  );
}
