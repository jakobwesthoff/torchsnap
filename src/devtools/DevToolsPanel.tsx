// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Developer Tools Panel
//
// Top-level layout for the Developer Tools window. Uses the
// shared `<TitleBar />` component in `"titlebar"` mode, which
// renders a classic macOS-style strip with a centered title
// and window controls at the top-left corner. The outer flex
// container reserves `pt-8` (32px) to match the titlebar
// overlay's height. See ADR 0034.
// =========================================================

import { useState } from "react";
import { DevToolsTabBar, type DevToolsTab } from "./DevToolsTabBar";
import { ConsoleTab } from "./console/ConsoleTab";
import { TitleBar } from "../components/TitleBar";

export function DevToolsPanel() {
  const [activeTab, setActiveTab] = useState<DevToolsTab>("console");

  return (
    <div className="relative flex flex-col h-screen pt-8 font-sans antialiased bg-surface text-text-primary">
      <TitleBar variant="titlebar" title="Developer Tools" />

      {/* Tab bar — sits just below the TitleBar strip */}
      <DevToolsTabBar activeTab={activeTab} onTabChange={setActiveTab} />

      {/* Tab content — fills remaining vertical space */}
      <div className="flex-1 min-h-0">{activeTab === "console" && <ConsoleTab />}</div>
    </div>
  );
}
