// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { SwatchIcon } from "@heroicons/react/24/outline";
import { useSetting } from "../../hooks/useSetting";
import { SectionHeader } from "../SectionHeader";
import { Section } from "../Section";
import { Entry } from "../Entry";
import { Switch } from "../../components/Switch";
import { ThemeToggle } from "../../components/ThemeToggle";

export function AppearanceSection() {
  const [mascotMode, setMascotMode] = useSetting<string>("mascotMode");
  const [randomMascots, setRandomMascots] = useSetting<boolean>("randomMascots");
  const [showNsfwMascots, setShowNsfwMascots] = useSetting<boolean>("showNsfwMascots");

  const mascotEnabled = mascotMode !== "off";

  return (
    <div className="flex flex-col gap-4">
      <SectionHeader
        icon={SwatchIcon}
        title="Appearance"
        description="Customize how Torchsnap looks — choose your preferred theme and configure the Snappy mascot to your liking."
      />

      <Section title="Appearance">
        <Entry label="Theme">
          <ThemeToggle />
        </Entry>
        <Entry label="Show Snappy mascot">
          <Switch checked={mascotEnabled} onChange={(on) => setMascotMode(on ? "center" : "off")} />
        </Entry>
        <Entry label="Snappy is only a Sidekick">
          <Switch
            checked={mascotMode === "sidekick"}
            onChange={(on) => setMascotMode(on ? "sidekick" : "center")}
            disabled={!mascotEnabled}
          />
        </Entry>
        <Entry
          label="Let Snappy do Cosplay"
          description="Show a different Snappy each time the launcher opens"
        >
          <Switch
            checked={randomMascots}
            onChange={(on) => setRandomMascots(on)}
            disabled={!mascotEnabled}
          />
        </Entry>
        <Entry
          label="Enable non family-friendly mascots"
          description="Some Snappy variants carry swords, daggers, or similar."
        >
          <Switch
            checked={showNsfwMascots}
            onChange={(on) => setShowNsfwMascots(on)}
            disabled={!mascotEnabled || !randomMascots}
          />
        </Entry>
      </Section>
    </div>
  );
}
