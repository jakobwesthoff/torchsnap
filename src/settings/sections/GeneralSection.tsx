// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { useEffect, useState } from "react";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useSetting } from "../../hooks/useSetting";
import { command } from "../../lib/command";
import { createLogger } from "../../lib/logger";
import { SectionHeader } from "../SectionHeader";
import { Section } from "../Section";
import { Entry } from "../Entry";
import { Switch } from "../../components/Switch";
import { ShortcutSection } from "../ShortcutSection";
import { UpdatesSection } from "../UpdatesSection";
import { SnappyEmblem } from "../../components/SnappyEmblem";

const WEBSITE = "https://torchsnap.app/";

const logger = createLogger("settings");

export function GeneralSection() {
  const [globalShortcut, setGlobalShortcut] = useSetting<string>("globalShortcut");

  const [controlApiEnabled, setControlApiEnabled] = useSetting<boolean>("controlChannel.enabled");

  const [launchAtLogin, setLaunchAtLogin] = useState(false);
  const [autoStartLoading, setAutoStartLoading] = useState(true);
  const [buildInfo, setBuildInfo] = useState<{ version: string; gitHash: string } | null>(null);

  useEffect(() => {
    command("build_info")
      .then(setBuildInfo)
      .catch((e: unknown) => logger.error(`reading build info failed: ${String(e)}`));
  }, []);

  // When the autostart state cannot be read, the switch starts off and
  // stays usable, so the user can still turn launching at login on.
  useEffect(() => {
    isEnabled()
      .then(setLaunchAtLogin)
      .catch((e: unknown) => logger.error(`reading launch at login failed: ${String(e)}`))
      .finally(() => setAutoStartLoading(false));
  }, []);

  // The switch moves at once and moves back when the change fails, so
  // it never shows a state the system does not have.
  const handleLaunchAtLoginChange = async (checked: boolean) => {
    setLaunchAtLogin(checked);
    try {
      if (checked) {
        await enable();
      } else {
        await disable();
      }
    } catch (e) {
      setLaunchAtLogin(!checked);
      logger.error(`changing launch at login failed: ${String(e)}`);
    }
  };

  return (
    <div className="flex flex-col gap-4 min-h-full">
      <SectionHeader
        icon="heroicons:cog-6-tooth"
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

      <UpdatesSection />

      <Section title="Advanced">
        <Entry label="Control API">
          <Switch checked={controlApiEnabled} onChange={setControlApiEnabled} />
        </Entry>
      </Section>

      <div className="mt-auto pt-4 self-end flex flex-col items-center gap-1">
        {/* The settings window has no navigation guard, so the click opens
            the site in the browser and must not follow the link itself. */}
        <a
          href={WEBSITE}
          title="torchsnap.app"
          className="group/emblem cursor-pointer"
          onClick={(event) => {
            event.preventDefault();
            void openUrl(WEBSITE).catch(() => {});
          }}
        >
          <SnappyEmblem width={96} lift alt="Open torchsnap.app" />
        </a>
        {buildInfo && (
          <p className="text-[11px] text-text-muted/90">
            Build: v{buildInfo.version} ({buildInfo.gitHash})
          </p>
        )}
      </div>
    </div>
  );
}
