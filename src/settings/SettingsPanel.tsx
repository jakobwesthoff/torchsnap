// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { Suspense, useMemo, useState } from "react";
import { getPluginSettingsComponent, getPluginsWithSettings } from "../plugins/registry";
import { createPluginSettingHook } from "../hooks/usePluginSetting";
import { createLogger } from "../lib/logger";
import { LoggerProvider } from "../lib/LoggerContext";
import { SettingsSidebar, type SidebarItem } from "./SettingsSidebar";
import { GeneralSection } from "./sections/GeneralSection";
import { AppearanceSection } from "./sections/AppearanceSection";
import { FrecencySection } from "./sections/FrecencySection";
import { WebsiteMetadataSection } from "./sections/WebsiteMetadataSection";
import { PluginSettingsWrapper } from "./PluginSettingsWrapper";

// =========================================================
// Built-in sidebar sections
// =========================================================

const BUILT_IN_SECTIONS: SidebarItem[] = [
  { id: "general", label: "General", settingsIcon: "heroicons:cog-6-tooth" },
  { id: "appearance", label: "Appearance", settingsIcon: "heroicons:swatch" },
  { id: "frecency", label: "Frecency", settingsIcon: "heroicons:chart-bar" },
  { id: "website-metadata", label: "Website Metadata", settingsIcon: "heroicons:globe-alt" },
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
  pluginSections: ReturnType<typeof getPluginsWithSettings>;
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

  // Plugin sections — wrapped in PluginSettingsWrapper for the
  // standardized header + enable/disable toggle. Custom settings
  // components (if any) are rendered as children.
  const plugin = pluginSections.find((p) => p.id === activeSection);
  if (!plugin) {
    return null;
  }

  return (
    <Suspense fallback={<div className="text-text-muted text-sm">Loading settings…</div>}>
      <PluginSectionContent plugin={plugin} />
    </Suspense>
  );
}

// =========================================================
// Plugin section content
//
// Renders the PluginSettingsWrapper (header + enable/disable)
// with an optional custom settings component as children.
// Memoizes the scoped hook factory per plugin ID.
// =========================================================

function PluginSectionContent({
  plugin,
}: {
  plugin: ReturnType<typeof getPluginsWithSettings>[number];
}) {
  const usePluginSetting = useMemo(() => createPluginSettingHook(plugin.id), [plugin.id]);
  const logger = useMemo(() => createLogger(plugin.id), [plugin.id]);

  const CustomSettings = getPluginSettingsComponent(plugin.id);

  return (
    <PluginSettingsWrapper
      pluginId={plugin.id}
      icon={plugin.icon ?? "heroicons:puzzle-piece"}
      name={plugin.label}
      description={plugin.description ?? ""}
    >
      {CustomSettings && (
        <LoggerProvider source={plugin.id}>
          <CustomSettings
            pluginId={plugin.id}
            usePluginSetting={usePluginSetting}
            logger={logger}
          />
        </LoggerProvider>
      )}
    </PluginSettingsWrapper>
  );
}
