// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Updates in Settings → General: the automatic-check switch, a
 * manual check (ADR 0053), and the way back to the welcome window.
 *
 * `updates.automaticChecks` has no default: it stays unset until the
 * user answers in the welcome window or flips this switch, and unset
 * means no automatic checks, which the switch shows as off.
 */

import type { ReactNode } from "react";
import { useSetting } from "../hooks/useSetting";
import { command } from "../lib/command";
import { Section } from "./Section";
import { Entry } from "./Entry";
import { Switch } from "../components/Switch";

const AUTOMATIC_CHECKS_KEY = "updates.automaticChecks";

export function UpdatesSection() {
  const [automaticChecks, setAutomaticChecks] = useSetting<boolean | undefined>(
    AUTOMATIC_CHECKS_KEY,
  );

  return (
    <Section title="Updates">
      <Entry
        label="Check for updates automatically"
        description="Once a day, Torchsnap asks torchsnap.app for the newest version. The request carries your IP address and nothing else about you."
      >
        <Switch checked={automaticChecks === true} onChange={setAutomaticChecks} />
      </Entry>
      <div className="flex justify-end gap-2">
        <QuietButton onClick={() => void command("welcome_show").catch(() => {})}>
          Show Welcome
        </QuietButton>
        <QuietButton onClick={() => void command("update_check").catch(() => {})}>
          Check for Updates
        </QuietButton>
      </div>
    </Section>
  );
}

function QuietButton({ onClick, children }: { onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="rounded-md px-3 py-1.5 text-sm font-medium text-text-secondary transition-colors hover:bg-surface-hover hover:text-text-primary"
    >
      {children}
    </button>
  );
}
