// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * A single result entry in the launcher list.
 *
 * Renders the entry icon, title (with fuzzy match highlighting),
 * optional subtitle, and responds to mouse/keyboard selection.
 */

import type { RefObject } from "react";
import { Icon } from "../components/Icon";
import { cn } from "../lib/cn";
import { highlightText } from "../lib/highlightText";
import type { SourcedEntry, EntryIcon } from "../types";

// =========================================================
// Icon Rendering
// =========================================================

/**
 * Convert the typed `EntryIcon` union (from Rust search results)
 * to the string identifier format used by the shared `Icon`
 * component.
 */
function entryIconToString(icon: EntryIcon): string {
  switch (icon.type) {
    case "heroIcon":
      return `heroicons:${icon.value}`;
    case "dataUrl":
      return `data:${icon.value}`;
    case "assetIcon":
      return `asset:${icon.value}`;
    case "emoji":
      return `emoji:${icon.value}`;
  }
}

/**
 * Renders the icon for a search result entry. Wraps the shared
 * `Icon` component with the launcher-specific 36×36 container
 * and sizing classes.
 */
function EntryIconView({ icon }: { icon: EntryIcon | null }) {
  if (!icon) {
    return (
      <div className="flex h-9 w-9 items-center justify-center">
        <Icon icon="heroicons:command-line" className="h-7 w-7 text-text-muted" />
      </div>
    );
  }

  const iconStr = entryIconToString(icon);

  // Asset icons fill the full container (no inner padding).
  if (icon.type === "assetIcon") {
    return <Icon icon={iconStr} className="h-9 w-9" />;
  }

  // Emoji uses its own text-based sizing.
  if (icon.type === "emoji") {
    return (
      <div className="flex h-9 w-9 items-center justify-center">
        <Icon icon={iconStr} className="text-2xl leading-none" />
      </div>
    );
  }

  // Default: centered 28×28 icon inside 36×36 container.
  return (
    <div className="flex h-9 w-9 items-center justify-center">
      <Icon icon={iconStr} className="h-7 w-7 text-text-secondary" />
    </div>
  );
}

// =========================================================
// Component
// =========================================================

interface ResultRowProps {
  entry: SourcedEntry;
  selected: boolean;
  onSelect: () => void;
  onExecute: () => void;
  mouseActiveRef: RefObject<boolean>;
}

export function ResultRow({
  entry,
  selected,
  onSelect,
  onExecute,
  mouseActiveRef,
}: ResultRowProps) {
  return (
    <div
      className={cn(
        "flex items-center gap-3 pl-3 pr-5 py-2.5 cursor-default border-l-2 border-transparent",
        selected && "bg-selection border-accent",
      )}
      onMouseEnter={() => {
        if (mouseActiveRef.current) {
          onSelect();
        }
      }}
      onClick={onExecute}
    >
      <div className="shrink-0">
        <EntryIconView icon={entry.icon} />
      </div>

      <div className="flex-1 min-w-0">
        <div className="text-sm text-text-primary truncate">
          {highlightText(entry.title, entry.titlePositions)}
        </div>
        {entry.subtitle && (
          <div className="text-xs text-text-muted truncate">
            {highlightText(entry.subtitle, entry.subtitlePositions)}
          </div>
        )}
      </div>
    </div>
  );
}
