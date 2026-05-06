// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { Icon } from "../components/Icon";
import { cn } from "../lib/cn";

// =========================================================
// Types
// =========================================================

export interface SidebarItem {
  id: string;
  label: string;
  /** String icon identifier (e.g. "heroicons:cog-6-tooth"). */
  icon?: string;
}

interface SettingsSidebarProps {
  /** Top-level items (General). Rendered without a group header. */
  generalItems: SidebarItem[];
  /** Customization group items (Appearance, Frecency, …). */
  customizationItems: SidebarItem[];
  /** Gadget-provided sections. */
  gadgetItems: SidebarItem[];
  /** Currently selected section ID. */
  activeId: string;
  /** Called when a section is selected. */
  onSelect: (id: string) => void;
}

// =========================================================
// Component
// =========================================================

export function SettingsSidebar({
  generalItems,
  customizationItems,
  gadgetItems,
  activeId,
  onSelect,
}: SettingsSidebarProps) {
  return (
    <nav className="flex flex-col py-2 px-2">
      <SidebarGroup items={generalItems} activeId={activeId} onSelect={onSelect} />

      {customizationItems.length > 0 && (
        <SidebarGroup
          label="Customization"
          items={customizationItems}
          activeId={activeId}
          onSelect={onSelect}
        />
      )}

      {gadgetItems.length > 0 && (
        <SidebarGroup label="Gadgets" items={gadgetItems} activeId={activeId} onSelect={onSelect} />
      )}
    </nav>
  );
}

// =========================================================
// Sidebar group — optional uppercase tracked header
// followed by its buttons. First group omits the header.
// =========================================================

function SidebarGroup({
  label,
  items,
  activeId,
  onSelect,
}: {
  label?: string;
  items: SidebarItem[];
  activeId: string;
  onSelect: (id: string) => void;
}) {
  return (
    <div className="flex flex-col gap-0.5 mb-2">
      {label && (
        <h4 className="text-[10px] font-semibold tracking-wider uppercase text-text-tertiary px-3 pt-3 pb-1">
          {label}
        </h4>
      )}
      {items.map((item) => (
        <SidebarButton
          key={item.id}
          item={item}
          active={item.id === activeId}
          onSelect={onSelect}
        />
      ))}
    </div>
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
  return (
    <button
      type="button"
      onClick={() => onSelect(item.id)}
      className={cn(
        "flex w-full items-center gap-2 rounded-lg px-3 py-1.5 text-left text-sm transition-colors",
        active ? "bg-accent text-white font-medium" : "text-text-primary hover:bg-surface-hover",
      )}
    >
      {item.icon && <Icon icon={item.icon} className="h-4 w-4 shrink-0" />}
      {item.label}
    </button>
  );
}
