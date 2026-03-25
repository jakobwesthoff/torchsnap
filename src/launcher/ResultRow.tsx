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
  CommandLineIcon,
} from "@heroicons/react/24/outline";
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
};

function EntryIconView({ icon }: { icon: EntryIcon | null }) {
  if (!icon) {
    // Fallback icon when no icon is specified.
    return <CommandLineIcon className="h-5 w-5 text-text-muted" />;
  }

  if (icon.type === "heroIcon") {
    const Icon = HERO_ICON_MAP[icon.value] ?? CommandLineIcon;
    return <Icon className="h-5 w-5 text-text-secondary" />;
  }

  if (icon.type === "dataUrl") {
    return (
      <img
        src={icon.value}
        alt=""
        className="h-5 w-5"
        draggable={false}
      />
    );
  }

  return <CommandLineIcon className="h-5 w-5 text-text-muted" />;
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
        "flex items-center gap-3 px-5 py-2.5 cursor-default",
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
