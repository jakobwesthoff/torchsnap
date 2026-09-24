// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Permission groups
//
// The one presentation of gadget permissions, used on the gadget
// cards and in the install review. Each group names what the
// gadget touches; broad groups carry a "Broad access" label whose
// tooltip explains it, so nothing depends on colour alone. When
// reviewing a replace, entries are marked New or Removed and a
// summary line says what changes.
// =========================================================

import { useState } from "react";
import { Icon } from "../../components/Icon";
import { cn } from "../../lib/cn";
import {
  changeSummary,
  groupPermissions,
  type GroupEntry,
  type PermissionGroup,
  type Platform,
} from "./permissionModel";
import type { PermissionItem } from "./types";

export const BROAD_ACCESS_HINT =
  "This reaches beyond the gadget's own data, for example running programs, contacting any website or opening files.";

export function PermissionGroups({
  items,
  removed = [],
  platform,
  showChanges = false,
  className,
}: {
  items: PermissionItem[];
  removed?: PermissionItem[];
  platform: Platform;
  /** Mark added and removed entries, for a replace review. */
  showChanges?: boolean;
  className?: string;
}) {
  const groups = groupPermissions(items, showChanges ? removed : [], platform);
  if (groups.length === 0) {
    return null;
  }
  const summary = showChanges ? changeSummary(items, removed) : null;
  const summaryText = summary && formatSummary(summary);

  return (
    <div className={cn("flex flex-col gap-2", className)}>
      {summaryText && <p className="text-xs text-text-secondary">{summaryText}</p>}
      {groups.map((group) => (
        <GroupView key={group.id} group={group} showChanges={showChanges} />
      ))}
    </div>
  );
}

function formatSummary({ adds, drops }: { adds: string[]; drops: string[] }): string | null {
  const parts = [
    adds.length > 0 ? `Adds: ${adds.join(", ")}` : null,
    drops.length > 0 ? `Drops: ${drops.join(", ")}` : null,
  ].filter(Boolean);
  return parts.length > 0 ? parts.join(" · ") : null;
}

function GroupView({ group, showChanges }: { group: PermissionGroup; showChanges: boolean }) {
  const [showOther, setShowOther] = useState(false);
  const titleId = `permission-group-${group.id}`;
  return (
    <div role="group" aria-labelledby={titleId} className="flex gap-2 text-xs">
      <Icon icon={group.icon} className="mt-0.5 h-3.5 w-3.5 shrink-0 text-text-tertiary" />
      <div className="flex min-w-0 flex-col gap-0.5">
        <div className="flex items-center gap-1.5">
          <span id={titleId} className="font-medium text-text-secondary">
            {group.title}
          </span>
          {group.broad && <BroadAccessTag />}
        </div>
        <ul className="flex flex-col gap-0.5">
          {group.entries.map((entry, index) => (
            <EntryView key={index} entry={entry} showChanges={showChanges} />
          ))}
          {showOther &&
            group.otherSystems.map((entry, index) => (
              <EntryView key={`other-${index}`} entry={entry} showChanges={showChanges} />
            ))}
        </ul>
        {group.otherSystems.length > 0 && !showOther && (
          <button
            type="button"
            onClick={() => setShowOther(true)}
            className="self-start text-text-tertiary hover:text-text-secondary"
          >
            +{group.otherSystems.length} for other systems
          </button>
        )}
      </div>
    </div>
  );
}

function EntryView({ entry, showChanges }: { entry: GroupEntry; showChanges: boolean }) {
  const removed = entry.change === "removed";
  return (
    <li className="flex flex-wrap items-center gap-x-1.5 text-text-tertiary">
      <span className={cn("break-all", removed && "line-through")}>{entry.text}</span>
      {entry.detail && <span>{entry.detail}</span>}
      {showChanges && entry.change === "added" && <ChangeTag>New</ChangeTag>}
      {showChanges && removed && <ChangeTag>Removed</ChangeTag>}
    </li>
  );
}

function ChangeTag({ children }: { children: React.ReactNode }) {
  return (
    <span className="rounded bg-accent/15 px-1 text-[10px] font-medium text-accent">
      {children}
    </span>
  );
}

export function BroadAccessTag() {
  return (
    <span
      title={BROAD_ACCESS_HINT}
      className="rounded bg-amber-500/15 px-1 text-[10px] font-medium text-amber-500"
    >
      Broad access
    </span>
  );
}
