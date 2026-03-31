// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Global shortcut configuration for the launcher toggle.
 *
 * Wraps the reusable `ShortcutRecorder`. When the user records a
 * new combo, we write it to the settings store. The backend's
 * shortcut reactor picks up the change and re-registers all
 * shortcuts automatically — no explicit Tauri command needed.
 */

import { useCallback, useState } from "react";
import { ShortcutRecorder } from "../components/ShortcutRecorder";
import { Section } from "./Section";
import { Entry } from "./Entry";

interface ShortcutSectionProps {
  globalShortcut: string;
  setGlobalShortcut: (v: string) => Promise<void>;
}

export function ShortcutSection({ globalShortcut, setGlobalShortcut }: ShortcutSectionProps) {
  const [error, setError] = useState<string | null>(null);

  const handleChange = useCallback(
    async (combo: string) => {
      try {
        await setGlobalShortcut(combo);
        setError(null);
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      }
    },
    [setGlobalShortcut],
  );

  return (
    <Section>
      <Entry label="Global Shortcut">
        <ShortcutRecorder value={globalShortcut} onChange={handleChange} />
      </Entry>
      {error && <p className="text-sm text-red-500">{error}</p>}
    </Section>
  );
}
