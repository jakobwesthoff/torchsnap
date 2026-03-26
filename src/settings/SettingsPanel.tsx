// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { cn } from "../lib/cn";
import { useSetting } from "../hooks/useSetting";
import { SETTINGS_DEFAULTS } from "../settingsDefaults";
import { SettingsSection } from "../components/SettingsSection";
import { SettingsEntry } from "../components/SettingsEntry";
import { Switch } from "../components/Switch";
import { ThemeToggle } from "../components/ThemeToggle";
import { ShortcutSection } from "./ShortcutSection";

export function SettingsPanel() {
  const [globalShortcut, setGlobalShortcut, shortcutReady] = useSetting(
    "globalShortcut",
    SETTINGS_DEFAULTS.globalShortcut,
  );

  const [showMascot, setShowMascot, mascotReady] = useSetting(
    "showMascot",
    SETTINGS_DEFAULTS.showMascot,
  );

  const loading = !shortcutReady || !mascotReady;

  return (
    <div
      className={cn(
        "min-h-screen font-sans antialiased",
        "bg-surface text-text-primary",
        loading ? "opacity-0" : "opacity-100 transition-opacity duration-300",
      )}
    >
      {/* Drag region — macOS overlay titlebar */}
      <div data-tauri-drag-region className="h-12 w-full select-none" />

      {/* Content */}
      <div className="px-6 pb-6">
        <h2 className="text-lg font-semibold mb-4">Settings</h2>

        <ShortcutSection
          globalShortcut={globalShortcut}
          setGlobalShortcut={setGlobalShortcut}
        />

        {/* Appearance */}
        <SettingsSection title="Appearance" className="mt-4">
          <SettingsEntry label="Theme">
            <ThemeToggle />
          </SettingsEntry>
          <SettingsEntry label="Show Snappy mascot">
            <Switch checked={showMascot} onChange={setShowMascot} />
          </SettingsEntry>
        </SettingsSection>
      </div>
    </div>
  );
}
