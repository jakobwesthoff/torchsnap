// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { useEffect, useState } from "react";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
import { useSetting } from "../../hooks/useSetting";
import { SettingsSection } from "../../components/SettingsSection";
import { SettingsEntry } from "../../components/SettingsEntry";
import { Switch } from "../../components/Switch";
import { ShortcutSection } from "../ShortcutSection";

export function GeneralSection() {
  const [globalShortcut, setGlobalShortcut] =
    useSetting<string>("globalShortcut");

  const [launchAtLogin, setLaunchAtLogin] = useState(false);
  const [autoStartLoading, setAutoStartLoading] = useState(true);

  useEffect(() => {
    isEnabled().then((enabled) => {
      setLaunchAtLogin(enabled);
      setAutoStartLoading(false);
    });
  }, []);

  const handleLaunchAtLoginChange = async (checked: boolean) => {
    setLaunchAtLogin(checked);
    if (checked) {
      await enable();
    } else {
      await disable();
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <SettingsSection title="Startup">
        <SettingsEntry label="Launch at login">
          <Switch
            checked={launchAtLogin}
            onChange={handleLaunchAtLoginChange}
            disabled={autoStartLoading}
          />
        </SettingsEntry>
      </SettingsSection>

      <ShortcutSection
        globalShortcut={globalShortcut}
        setGlobalShortcut={setGlobalShortcut}
      />
    </div>
  );
}
