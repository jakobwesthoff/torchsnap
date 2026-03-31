// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { useEffect, useState } from "react";
import { Cog6ToothIcon } from "@heroicons/react/24/outline";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
import { useSetting } from "../../hooks/useSetting";
import { SectionHeader } from "../SectionHeader";
import { Section } from "../Section";
import { Entry } from "../Entry";
import { Switch } from "../../components/Switch";
import { ShortcutSection } from "../ShortcutSection";

export function GeneralSection() {
  const [globalShortcut, setGlobalShortcut] = useSetting<string>("globalShortcut");

  const [controlApiEnabled, setControlApiEnabled] = useSetting<boolean>("controlChannel.enabled");

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
      <SectionHeader
        icon={Cog6ToothIcon}
        title="General"
        description="Core settings for Torchsnap — configure startup behavior, global keyboard shortcuts, and advanced features like the Control API."
      />

      <Section title="Startup">
        <Entry label="Launch at login">
          <Switch
            checked={launchAtLogin}
            onChange={handleLaunchAtLoginChange}
            disabled={autoStartLoading}
          />
        </Entry>
      </Section>

      <ShortcutSection globalShortcut={globalShortcut} setGlobalShortcut={setGlobalShortcut} />

      <Section title="Advanced">
        <Entry label="Control API">
          <Switch checked={controlApiEnabled} onChange={setControlApiEnabled} />
        </Entry>
      </Section>
    </div>
  );
}
