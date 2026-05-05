// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Generic settings wrapper rendered for every gadget that appears
 * in the settings sidebar. Provides the standardized layout:
 *
 * 1. Section header (icon, name, description)
 * 2. Enable/disable toggle (reading `enabled.<gadgetId>`)
 * 3. Gadget-specific settings as children (optional)
 *
 * The enable/disable key lives at the top level of the settings
 * store (`enabled.<gadgetId>`), separate from the gadget's own
 * namespace (`gadgets.<gadgetId>.*`). This ensures gadgets cannot
 * access their own enabled state.
 */

import type { ReactNode } from "react";
import { useSetting } from "../hooks/useSetting";
import { SectionHeader } from "./SectionHeader";
import { Section } from "./Section";
import { Entry } from "./Entry";
import { Switch } from "../components/Switch";

interface GadgetSettingsWrapperProps {
  gadgetId: string;
  icon: string;
  name: string;
  description: string;
  children?: ReactNode;
}

export function GadgetSettingsWrapper({
  gadgetId,
  icon,
  name,
  description,
  children,
}: GadgetSettingsWrapperProps) {
  const [enabled, setEnabled] = useSetting<boolean>(`enabled.${gadgetId}`);

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
