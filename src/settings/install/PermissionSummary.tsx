// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Permission summary
//
// The list of what a gadget may do, shared by the install review
// and the installed gadget cards. Broad grants come first and are
// highlighted; the review additionally marks what a new version
// adds and lists what it drops.
// =========================================================

import { cn } from "../../lib/cn";
import { permissionText } from "./permissionText";
import type { PermissionItem, Severity } from "./types";

const SEVERITY_ORDER: Record<Severity, number> = { warning: 0, notice: 1, info: 2 };

const SEVERITY_STYLE: Record<Severity, string> = {
  warning: "border-l-amber-500 text-text-primary",
  notice: "border-l-accent/60 text-text-primary",
  info: "border-l-border-divider text-text-secondary",
};

export function PermissionSummary({
  items,
  removed = [],
  showChanges = false,
  className,
}: {
  items: PermissionItem[];
  removed?: PermissionItem[];
  /** Mark added items and list removed ones, for a replace review. */
  showChanges?: boolean;
  className?: string;
}) {
  if (items.length === 0 && removed.length === 0) {
    return null;
  }

  const sorted = [...items].sort((a, b) => SEVERITY_ORDER[a.severity] - SEVERITY_ORDER[b.severity]);

  return (
    <div className={cn("flex flex-col gap-2", className)}>
      {sorted.length > 0 && (
        <ul aria-label="Permissions" className="flex flex-col gap-1">
          {sorted.map((item, index) => (
            <PermissionRow key={index} item={item} markAdded={showChanges} />
          ))}
        </ul>
      )}
      {showChanges && removed.length > 0 && (
        <>
          <div className="text-xs font-medium text-text-tertiary">No longer requested</div>
          <ul aria-label="No longer requested" className="flex flex-col gap-1 opacity-70">
            {removed.map((item, index) => (
              <PermissionRow key={index} item={item} markAdded={false} />
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

function PermissionRow({ item, markAdded }: { item: PermissionItem; markAdded: boolean }) {
  const { title, detail } = permissionText(item.permission);
  return (
    <li
      data-severity={item.severity}
      className={cn("border-l-2 pl-2 text-xs", SEVERITY_STYLE[item.severity])}
    >
      <div className="flex items-center gap-1.5">
        <span className={item.change === "removed" ? "line-through" : undefined}>{title}</span>
        {markAdded && item.change === "added" && (
          <span className="rounded bg-amber-500/15 px-1 text-[10px] font-medium uppercase text-amber-500">
            New
          </span>
        )}
      </div>
      {detail && <div className="text-text-tertiary">{detail}</div>}
    </li>
  );
}
