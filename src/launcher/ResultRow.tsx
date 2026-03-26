// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * A single result entry in the launcher list.
 *
 * Renders the entry icon, title (with fuzzy match highlighting),
 * optional subtitle, and responds to mouse/keyboard selection.
 */

import type { ReactNode, RefObject } from "react";
import {
  XCircleIcon,
  Cog6ToothIcon,
  SunIcon,
  RocketLaunchIcon,
  CommandLineIcon,
} from "@heroicons/react/24/outline";
import { convertFileSrc } from "@tauri-apps/api/core";
import { cn } from "../lib/cn";
import type { ScoredEntry, EntryIcon } from "./types";

// =========================================================
// Icon Rendering
// =========================================================

// Static lookup for the HeroIcon names used by built-in plugins.
// Add entries here as new plugins are registered with HeroIcon icons.
const HERO_ICON_MAP: Record<
  string,
  React.ComponentType<React.SVGProps<SVGSVGElement>>
> = {
  "x-circle": XCircleIcon,
  "cog-6-tooth": Cog6ToothIcon,
  sun: SunIcon,
  "rocket-launch": RocketLaunchIcon,
};

function EntryIconView({ icon }: { icon: EntryIcon | null }) {
  if (!icon) {
    return (
      <div className="flex h-9 w-9 items-center justify-center">
        <CommandLineIcon className="h-7 w-7 text-text-muted" />
      </div>
    );
  }

  if (icon.type === "heroIcon") {
    const Icon = HERO_ICON_MAP[icon.value] ?? CommandLineIcon;
    return (
      <div className="flex h-9 w-9 items-center justify-center">
        <Icon className="h-7 w-7 text-text-secondary" />
      </div>
    );
  }

  if (icon.type === "dataUrl") {
    return (
      <div className="flex h-9 w-9 items-center justify-center">
        <img
          src={icon.value}
          alt=""
          className="h-7 w-7"
          draggable={false}
        />
      </div>
    );
  }

  if (icon.type === "assetIcon") {
    return (
      <img
        src={convertFileSrc(icon.value)}
        alt=""
        className="h-9 w-9"
        draggable={false}
      />
    );
  }

  return (
    <div className="flex h-9 w-9 items-center justify-center">
      <CommandLineIcon className="h-7 w-7 text-text-muted" />
    </div>
  );
}

// =========================================================
// Match Highlighting
// =========================================================

/**
 * Split text at matched character boundaries and wrap matched
 * runs in a highlighted span.
 *
 * Positions are character indices from nucleo. For ASCII text
 * these map 1:1 to JS string indices.
 */
// TODO: For non-ASCII (emoji, CJK), nucleo returns grapheme
// indices that may differ from JS string indices. Add a
// conversion layer when needed.
function highlightText(text: string, positions: number[]): ReactNode[] {
  if (positions.length === 0) return [text];

  const posSet = new Set(positions);
  const segments: ReactNode[] = [];
  let current = "";
  let inMatch = false;

  for (let i = 0; i < text.length; i++) {
    const isMatch = posSet.has(i);
    if (isMatch !== inMatch) {
      if (current) {
        segments.push(
          inMatch ? (
            <span key={`m${i}`} className="text-accent font-semibold">
              {current}
            </span>
          ) : (
            <span key={`t${i}`}>{current}</span>
          ),
        );
      }
      current = "";
      inMatch = isMatch;
    }
    current += text[i];
  }

  if (current) {
    segments.push(
      inMatch ? (
        <span key="me" className="text-accent font-semibold">
          {current}
        </span>
      ) : (
        <span key="te">{current}</span>
      ),
    );
  }

  return segments;
}

// =========================================================
// Component
// =========================================================

interface ResultRowProps {
  entry: ScoredEntry;
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
        "flex items-center gap-3 pl-3 pr-5 py-2.5 cursor-default",
        selected && "bg-accent/10",
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
