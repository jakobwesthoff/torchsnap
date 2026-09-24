// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Updates in Settings → General: the automatic-check switch and a
 * manual check (ADR 0053).
 *
 * `updates.automaticChecks` has no default: it stays unset until the
 * user answers in the welcome window or flips this switch, and unset
 * means no automatic checks, which the switch shows as off.
 */

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
      <div className="flex justify-end">
        <button
          type="button"
          onClick={() => void command("update_check").catch(() => {})}
          className="rounded-md px-3 py-1.5 text-sm font-medium text-text-secondary transition-colors hover:bg-surface-hover hover:text-text-primary"
        >
          Check for Updates
        </button>
      </div>
    </Section>
  );
}
