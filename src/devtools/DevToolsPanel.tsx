// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Developer Tools Panel
//
// Top-level layout for the Developer Tools window. A custom
// `<TitleBar />` overlays the top 48px as an absolute chrome
// layer (see ADR 0034); the outer flex container uses `pt-12`
// to reserve that space so the tab bar and console render
// cleanly below it.
// =========================================================

import { useState } from "react";
import { DevToolsTabBar, type DevToolsTab } from "./DevToolsTabBar";
import { ConsoleTab } from "./console/ConsoleTab";
import { TitleBar } from "../components/TitleBar";

export function DevToolsPanel() {
  const [activeTab, setActiveTab] = useState<DevToolsTab>("console");

  return (
    <div className="relative flex flex-col h-screen pt-12 font-sans antialiased bg-surface text-text-primary">
      <TitleBar />

      {/* Tab bar — sits just below the TitleBar overlay */}
      <DevToolsTabBar activeTab={activeTab} onTabChange={setActiveTab} />

      {/* Tab content — fills remaining vertical space */}
      <div className="flex-1 min-h-0">{activeTab === "console" && <ConsoleTab />}</div>
    </div>
  );
}
