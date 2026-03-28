// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Global shortcut configuration for the launcher toggle.
 *
 * Wraps the reusable `ShortcutRecorder` and handles persisting
 * the new combo to the settings store and re-registering with
 * the OS via the `update_global_shortcut` Tauri command.
 */

import { useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ShortcutRecorder } from "../components/ShortcutRecorder";
import { SettingsSection } from "../components/SettingsSection";
import { SettingsEntry } from "../components/SettingsEntry";

interface ShortcutSectionProps {
  globalShortcut: string;
  setGlobalShortcut: (v: string) => Promise<void>;
}

export function ShortcutSection({
  globalShortcut,
  setGlobalShortcut,
}: ShortcutSectionProps) {
  const [error, setError] = useState<string | null>(null);

  const handleChange = useCallback(
    async (combo: string) => {
      try {
        await invoke("update_global_shortcut", { shortcut: combo });
        await setGlobalShortcut(combo);
        setError(null);
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      }
    },
    [setGlobalShortcut],
  );

  return (
    <SettingsSection>
      <SettingsEntry label="Global Shortcut">
        <ShortcutRecorder value={globalShortcut} onChange={handleChange} />
      </SettingsEntry>
      {error && (
        <p className="text-sm text-red-500">{error}</p>
      )}
    </SettingsSection>
  );
}
