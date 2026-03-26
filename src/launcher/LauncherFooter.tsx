// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Contextual action footer for the launcher.
 *
 * Renders a generic `FooterState`: primary hint on the left,
 * secondary hints on the right. Both the host (deriving from
 * entry actions) and plugin custom UIs (setting state directly)
 * produce the same `FooterState` shape.
 */

import { KeyBindingPill } from "../components/KeyBindingPill";
import type { FooterState } from "./types";

interface LauncherFooterProps {
  footer: FooterState;
}

export function LauncherFooter({ footer }: LauncherFooterProps) {
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
            <span
              key={i}
              className="inline-flex items-center gap-1.5 text-xs text-text-muted"
            >
              {hint.combo && (
                <KeyBindingPill
                  modifiers={hint.combo.modifiers}
                  keyName={hint.combo.key}
                />
              )}
              <span>{hint.label}</span>
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
