// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Generic settings wrapper rendered for every plugin that appears
 * in the settings sidebar. Provides the standardized layout:
 *
 * 1. Section header (icon, name, description)
 * 2. Enable/disable toggle (reading `enabled.<pluginId>`)
 * 3. Plugin-specific settings as children (optional)
 *
 * The enable/disable key lives at the top level of the settings
 * store (`enabled.<pluginId>`), separate from the plugin's own
 * namespace (`plugins.<pluginId>.*`). This ensures plugins cannot
 * access their own enabled state.
 */

import type { ReactNode } from "react";
import { useSetting } from "../hooks/useSetting";
import { SectionHeader } from "./SectionHeader";
import { Section } from "./Section";
import { Entry } from "./Entry";
import { Switch } from "../components/Switch";

interface PluginSettingsWrapperProps {
  pluginId: string;
  icon: string;
  name: string;
  description: string;
  children?: ReactNode;
}

export function PluginSettingsWrapper({
  pluginId,
  icon,
  name,
  description,
  children,
}: PluginSettingsWrapperProps) {
  const [enabled, setEnabled] = useSetting<boolean>(`enabled.${pluginId}`);

  return (
    <div className="flex flex-col gap-4">
      <SectionHeader icon={icon} title={name} description={description} />
      <Section>
        <Entry label={`Enable ${name}`}>
          <Switch checked={enabled} onChange={setEnabled} />
        </Entry>
      </Section>
      {children}
    </div>
  );
}
