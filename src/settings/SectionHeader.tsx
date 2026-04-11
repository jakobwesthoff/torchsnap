// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Header displayed at the top of each settings section.
 *
 * Shows an accent-tinted icon tile next to the section title and a
 * short explanatory description, rendered inline on the content
 * surface without a container card.
 */

import { Icon } from "../components/Icon";

interface SectionHeaderProps {
  /** String identifier for the icon (e.g. "heroicons:cog-6-tooth"). */
  icon: string;
  title: string;
  description: string;
}

export function SectionHeader({ icon, title, description }: SectionHeaderProps) {
  return (
    <div className="flex items-center gap-4 mb-6">
      <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-accent/10">
        <Icon icon={icon} className="h-5 w-5 text-accent" />
      </div>
      <div className="flex flex-col gap-0.5">
        <h2 className="text-sm font-semibold text-text-primary">{title}</h2>
        <p className="text-xs leading-relaxed text-text-secondary">{description}</p>
      </div>
    </div>
  );
}
