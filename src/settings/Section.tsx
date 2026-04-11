// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * A visually grouped section in the settings panel.
 *
 * Renders an uppercase tracked header (when provided) above a
 * borderless rounded container filled with `--color-surface-group`.
 * The header sits outside the fill so the two signals — typography
 * and surface — reinforce each other.
 */

import type { ReactNode } from "react";
import { cn } from "../lib/cn";

interface SectionProps {
  title?: string;
  children: ReactNode;
  className?: string;
}

export function Section({ title, children, className }: SectionProps) {
  return (
    <section className={cn("flex flex-col", className)}>
      {title && (
        <h3 className="text-[11px] font-semibold tracking-wider uppercase text-text-tertiary px-1 mb-2">
          {title}
        </h3>
      )}
      <div className="rounded-xl bg-surface-group p-4 flex flex-col gap-3">{children}</div>
    </section>
  );
}
