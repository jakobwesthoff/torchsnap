// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { useSetting } from "../../hooks/useSetting";
import { SettingsSection } from "../../components/SettingsSection";
import { SettingsEntry } from "../../components/SettingsEntry";
import { Switch } from "../../components/Switch";
import { ThemeToggle } from "../../components/ThemeToggle";

export function AppearanceSection() {
  const [mascotMode, setMascotMode] = useSetting<string>("mascotMode");

  const mascotEnabled = mascotMode !== "off";

  return (
    <SettingsSection title="Appearance">
      <SettingsEntry label="Theme">
        <ThemeToggle />
      </SettingsEntry>
      <SettingsEntry label="Show Snappy mascot">
        <Switch checked={mascotEnabled} onChange={(on) => setMascotMode(on ? "center" : "off")} />
      </SettingsEntry>
      <SettingsEntry label="Snappy is only a Sidekick">
        <Switch
          checked={mascotMode === "sidekick"}
          onChange={(on) => setMascotMode(on ? "sidekick" : "center")}
          disabled={!mascotEnabled}
        />
      </SettingsEntry>
    </SettingsSection>
  );
}
