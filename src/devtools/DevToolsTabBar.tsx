// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { cn } from "../lib/cn";

export type DevToolsTab = "console";

interface DevToolsTabBarProps {
  activeTab: DevToolsTab;
  onTabChange: (tab: DevToolsTab) => void;
}

const tabs: { id: DevToolsTab; label: string }[] = [
  { id: "console", label: "Console" },
];

export function DevToolsTabBar({ activeTab, onTabChange }: DevToolsTabBarProps) {
  return (
    <div className="flex items-end h-12 pl-[80px] pr-4 border-b border-border">
      {tabs.map((tab) => (
        <button
          key={tab.id}
          onClick={() => onTabChange(tab.id)}
          className={cn(
            "px-4 py-2 text-xs font-medium tracking-wide uppercase transition-colors",
            "border-b-2 -mb-px",
            tab.id === activeTab
              ? "border-accent text-text-primary"
              : "border-transparent text-text-tertiary hover:text-text-secondary hover:border-border-hover",
          )}
        >
          {tab.label}
        </button>
      ))}
    </div>
  );
}
