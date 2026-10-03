// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Contextual action footer for the launcher.
 *
 * Renders a generic `FooterState`: primary hint on the left,
 * secondary hints on the right. Both the host (deriving from
 * entry actions) and gadget custom UIs (setting state directly)
 * produce the same `FooterState` shape.
 *
 * While a launcher action has failed, one error line takes the
 * place of the hints (ADR 62). It is cut to one line; the tooltip
 * holds the full text.
 */

import { ExclamationTriangleIcon } from "@heroicons/react/24/outline";
import { KeyBindingPill } from "../components/KeyBindingPill";
import type { FooterState } from "../types";

interface LauncherFooterProps {
  footer: FooterState;
  error?: string | null;
}

export function LauncherFooter({ footer, error }: LauncherFooterProps) {
  if (error) {
    return (
      <div
        role="alert"
        title={error}
        className="flex items-center gap-1.5 border-t border-border px-5 py-2.5 text-xs text-red-500"
      >
        <ExclamationTriangleIcon className="h-4 w-4 shrink-0" />
        {/* `leading-5` matches the key caps, so the footer keeps its height. */}
        <span className="truncate leading-5">{error}</span>
      </div>
    );
  }

  if (!footer.primary && footer.hints.length === 0) return null;

  return (
    <div className="flex items-center justify-between border-t border-border px-5 py-2.5">
      {/* Primary hint — always on the left */}
      {footer.primary && (
        <span className="inline-flex items-center gap-1.5 text-xs text-text-muted">
          {footer.primary.combo && (
            <KeyBindingPill
              modifiers={footer.primary.combo.modifiers}
              keyName={footer.primary.combo.key}
            />
          )}
          <span>{footer.primary.label}</span>
        </span>
      )}

      {/* Secondary hints — grouped on the right */}
      {footer.hints.length > 0 && (
        <div className="flex items-center gap-4">
          {footer.hints.map((hint, i) => (
            <span key={i} className="inline-flex items-center gap-1.5 text-xs text-text-muted">
              {hint.combo && (
                <KeyBindingPill modifiers={hint.combo.modifiers} keyName={hint.combo.key} />
              )}
              <span>{hint.label}</span>
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
