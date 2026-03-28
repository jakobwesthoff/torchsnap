// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import type { ComponentType, SVGProps } from "react";
import { cn } from "../lib/cn";

// =========================================================
// Types
// =========================================================

export interface SidebarItem {
  id: string;
  label: string;
  settingsIcon?: ComponentType<SVGProps<SVGSVGElement>>;
}

interface SettingsSidebarProps {
  /** Built-in sections (General, Appearance). */
  builtInItems: SidebarItem[];
  /** Plugin-provided sections. */
  pluginItems: SidebarItem[];
  /** Currently selected section ID. */
  activeId: string;
  /** Called when a section is selected. */
  onSelect: (id: string) => void;
}

// =========================================================
// Component
// =========================================================

export function SettingsSidebar({
  builtInItems,
  pluginItems,
  activeId,
  onSelect,
}: SettingsSidebarProps) {
  return (
    <nav className="flex flex-col gap-0.5 py-2 px-2">
      {builtInItems.map((item) => (
        <SidebarButton
          key={item.id}
          item={item}
          active={item.id === activeId}
          onSelect={onSelect}
        />
      ))}

      {pluginItems.length > 0 && (
        <>
          <div className="mx-2 my-1.5 border-t border-border-divider" />
          {pluginItems.map((item) => (
            <SidebarButton
              key={item.id}
              item={item}
              active={item.id === activeId}
              onSelect={onSelect}
            />
          ))}
        </>
      )}
    </nav>
  );
}

// =========================================================
// Sidebar button
// =========================================================

function SidebarButton({
  item,
  active,
  onSelect,
}: {
  item: SidebarItem;
  active: boolean;
  onSelect: (id: string) => void;
}) {
  const Icon = item.settingsIcon;

  return (
    <button
      type="button"
      onClick={() => onSelect(item.id)}
      className={cn(
        "flex w-full items-center gap-2 rounded-lg px-3 py-1.5 text-left text-sm transition-colors",
        active ? "bg-accent text-white font-medium" : "text-text-primary hover:bg-surface-hover",
      )}
    >
      {Icon && <Icon className="h-4 w-4 shrink-0" />}
      {item.label}
    </button>
  );
}
