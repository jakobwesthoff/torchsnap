// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { type ComponentType, Suspense, useMemo, useState } from "react";
import { getPluginSettingsComponent, getPluginsWithSettings } from "../plugins/registry";
import type { PluginSettingsProps } from "../plugins/types";
import { createLogger } from "../lib/logger";
import { useSetting } from "../hooks/useSetting";
import { PluginContextProvider } from "../contexts/PluginContextProvider";
import type { PluginInfo, PluginRuntime } from "../contexts/PluginContext";
import { sendPluginMessage } from "../lib/pluginMessage";
import { SettingsSidebar, type SidebarItem } from "./SettingsSidebar";
import { GeneralSection } from "./sections/GeneralSection";
import { AppearanceSection } from "./sections/AppearanceSection";
import { FrecencySection } from "./sections/FrecencySection";
import { WebsiteMetadataSection } from "./sections/WebsiteMetadataSection";
import { PluginSettingsWrapper } from "./PluginSettingsWrapper";

// =========================================================
// Built-in sidebar sections
// =========================================================

const GENERAL_SECTIONS: SidebarItem[] = [
  { id: "general", label: "General", icon: "heroicons:cog-6-tooth" },
];

const CUSTOMIZATION_SECTIONS: SidebarItem[] = [
  { id: "appearance", label: "Appearance", icon: "heroicons:swatch" },
  { id: "frecency", label: "Frecency", icon: "heroicons:chart-bar" },
  { id: "website-metadata", label: "Website Metadata", icon: "heroicons:globe-alt" },
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

      {/* Sidebar — two-tone surface + inset edge shadow fakes the
          macOS sidebar material without requiring real vibrancy. */}
      <aside
        className="w-[200px] shrink-0 bg-surface-sidebar pt-12 overflow-y-auto"
        style={{ boxShadow: "var(--sidebar-shadow)" }}
      >
        <SettingsSidebar
          generalItems={GENERAL_SECTIONS}
          customizationItems={CUSTOMIZATION_SECTIONS}
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

  // Stable reference — registry returns the same component instance per ID.
  const CustomSettings = getPluginSettingsComponent(plugin.id);

  return (
    <Suspense fallback={<div className="text-text-muted text-sm">Loading settings…</div>}>
      <PluginSectionContent plugin={plugin} CustomSettings={CustomSettings} />
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
  CustomSettings,
}: {
  plugin: ReturnType<typeof getPluginsWithSettings>[number];
  CustomSettings?: ComponentType<PluginSettingsProps>;
}) {
  // The plugin's enabled flag flows through PluginContext so
  // setting components can read it via usePluginInfo() and
  // visually disable controls when the plugin is off.
  const [enabled] = useSetting<boolean>(`enabled.${plugin.id}`);
  const logger = useMemo(() => createLogger(plugin.id), [plugin.id]);

  // sendMessage is bound to the active plugin id; the closure
  // never changes for a given mount.
  const sendMessage = useMemo(
    () =>
      <TPayload = unknown, TResult = unknown, TStream = never>(
        method: string,
        payload: TPayload,
        onMessage?: (msg: TStream) => void,
      ): Promise<TResult> =>
        sendPluginMessage<TPayload, TResult, TStream>(plugin.id, method, payload, onMessage),
    [plugin.id],
  );

  const info = useMemo<PluginInfo>(() => ({ id: plugin.id, enabled }), [plugin.id, enabled]);
  const runtime = useMemo<PluginRuntime>(() => ({ sendMessage, logger }), [sendMessage, logger]);

  return (
    <PluginSettingsWrapper
      pluginId={plugin.id}
      icon={plugin.icon ?? "heroicons:puzzle-piece"}
      name={plugin.label}
      description={plugin.description ?? ""}
    >
      {CustomSettings && (
        <PluginContextProvider info={info} runtime={runtime}>
          <CustomSettings />
        </PluginContextProvider>
      )}
    </PluginSettingsWrapper>
  );
}
