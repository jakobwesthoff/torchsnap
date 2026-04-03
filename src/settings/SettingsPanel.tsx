// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { Suspense, useMemo, useState } from "react";
import { ChartBarIcon, Cog6ToothIcon, GlobeAltIcon, SwatchIcon } from "@heroicons/react/24/outline";
import { getPluginSettingsComponent, getPluginsWithSettings } from "../plugins/registry";
import { createPluginSettingHook } from "../hooks/usePluginSetting";
import { SettingsSidebar, type SidebarItem } from "./SettingsSidebar";
import { GeneralSection } from "./sections/GeneralSection";
import { AppearanceSection } from "./sections/AppearanceSection";
import { FrecencySection } from "./sections/FrecencySection";
import { WebsiteMetadataSection } from "./sections/WebsiteMetadataSection";

// =========================================================
// Built-in sidebar sections
// =========================================================

const BUILT_IN_SECTIONS: SidebarItem[] = [
  { id: "general", label: "General", settingsIcon: Cog6ToothIcon },
  { id: "appearance", label: "Appearance", settingsIcon: SwatchIcon },
  { id: "frecency", label: "Frecency", settingsIcon: ChartBarIcon },
  { id: "website-metadata", label: "Website Metadata", settingsIcon: GlobeAltIcon },
];

// =========================================================
// Settings Panel
// =========================================================

export function SettingsPanel() {
  const [activeSection, setActiveSection] = useState("general");

  // Discover which plugins have settings components. This is
  // evaluated once per mount — plugins are registered statically.
  const pluginSections = useMemo(() => getPluginsWithSettings(), []);

  return (
    <div className="flex h-screen font-sans antialiased bg-surface text-text-primary">
      {/* Drag region — macOS overlay titlebar. Covers the full width
          so the user can drag from anywhere along the top edge. */}
      <div data-tauri-drag-region className="absolute inset-x-0 top-0 h-12 select-none z-10" />

      {/* Sidebar */}
      <aside className="w-[200px] shrink-0 border-r border-border pt-12 overflow-y-auto">
        <SettingsSidebar
          builtInItems={BUILT_IN_SECTIONS}
          pluginItems={pluginSections}
          activeId={activeSection}
          onSelect={setActiveSection}
        />
      </aside>

      {/* Content area */}
      <main className="flex-1 overflow-y-auto pt-12 px-6 pb-6">
        <SectionContent activeSection={activeSection} pluginSections={pluginSections} />
      </main>
    </div>
  );
}

// =========================================================
// Section content router
// =========================================================

function SectionContent({
  activeSection,
  pluginSections,
}: {
  activeSection: string;
  pluginSections: Array<{ id: string; label: string }>;
}) {
  // Built-in sections
  if (activeSection === "general") {
    return <GeneralSection />;
  }
  if (activeSection === "appearance") {
    return <AppearanceSection />;
  }
  if (activeSection === "frecency") {
    return <FrecencySection />;
  }
  if (activeSection === "website-metadata") {
    return <WebsiteMetadataSection />;
  }

  // Plugin sections — look up the settings component and inject
  // the scoped usePluginSetting hook.
  const plugin = pluginSections.find((p) => p.id === activeSection);
  if (!plugin) {
    return null;
  }

  const SettingsComponent = getPluginSettingsComponent(plugin.id);
  if (!SettingsComponent) {
    return null;
  }

  return (
    <Suspense fallback={<div className="text-text-muted text-sm">Loading settings…</div>}>
      <PluginSectionWrapper pluginId={plugin.id} Component={SettingsComponent} />
    </Suspense>
  );
}

// =========================================================
// Plugin section wrapper
//
// Memoizes the scoped hook factory per plugin ID so it stays
// referentially stable across re-renders.
// =========================================================

function PluginSectionWrapper({
  pluginId,
  Component,
}: {
  pluginId: string;
  Component: React.ComponentType<{
    pluginId: string;
    usePluginSetting: ReturnType<typeof createPluginSettingHook>;
  }>;
}) {
  const usePluginSetting = useMemo(() => createPluginSettingHook(pluginId), [pluginId]);

  return <Component pluginId={pluginId} usePluginSetting={usePluginSetting} />;
}
