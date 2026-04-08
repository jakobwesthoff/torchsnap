// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Developer Tools Panel
//
// Top-level layout for the Developer Tools window. The drag
// region is a separate visual strip at the top (housing the
// macOS traffic lights), with the tab bar below it left-
// aligned to the window edge.
// =========================================================

import { useState } from "react";
import { DevToolsTabBar, type DevToolsTab } from "./DevToolsTabBar";
import { ConsoleTab } from "./console/ConsoleTab";

export function DevToolsPanel() {
  const [activeTab, setActiveTab] = useState<DevToolsTab>("console");

  return (
    <div className="flex flex-col h-screen font-sans antialiased bg-surface text-text-primary">
      {/* Drag region — separate strip for traffic lights */}
      <div
        data-tauri-drag-region
        className="shrink-0 h-[30px] bg-surface-inset/50 border-b border-border-divider"
      />

      {/* Tab bar — below drag region, left-aligned */}
      <DevToolsTabBar activeTab={activeTab} onTabChange={setActiveTab} />

      {/* Tab content — fills remaining vertical space */}
      <div className="flex-1 min-h-0">{activeTab === "console" && <ConsoleTab />}</div>
    </div>
  );
}
