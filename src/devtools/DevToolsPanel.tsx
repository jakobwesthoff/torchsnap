// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Developer Tools Panel
//
// Top-level layout for the Developer Tools window. Uses the
// macOS overlay titlebar pattern (48px drag region) with a
// tab bar for switching between tool panels.
//
// Currently only the Console tab is implemented. The tab bar
// structure allows future tabs (Network, State, etc.) without
// layout changes.
// =========================================================

import { useState } from "react";
import { DevToolsTabBar, type DevToolsTab } from "./DevToolsTabBar";

export function DevToolsPanel() {
  const [activeTab, setActiveTab] = useState<DevToolsTab>("console");

  return (
    <div className="flex flex-col h-screen font-sans antialiased bg-surface text-text-primary">
      {/* macOS overlay titlebar drag region */}
      <div
        data-tauri-drag-region
        className="absolute inset-x-0 top-0 h-12 select-none z-10"
      />

      {/* Tab bar — sits within the drag region height */}
      <DevToolsTabBar activeTab={activeTab} onTabChange={setActiveTab} />

      {/* Tab content — fills remaining vertical space */}
      <div className="flex-1 min-h-0">
        {activeTab === "console" && (
          <div className="flex items-center justify-center h-full">
            <p className="text-sm text-text-muted">Console placeholder</p>
          </div>
        )}
      </div>
    </div>
  );
}
