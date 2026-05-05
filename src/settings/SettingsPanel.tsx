// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { type ComponentType, Suspense, useMemo, useState } from "react";
import { getGadgetSettingsComponent, getGadgetsWithSettings } from "../plugins/registry";
import type { GadgetSettingsProps } from "../plugins/types";
import { createLogger } from "../lib/logger";
import { useSetting } from "../hooks/useSetting";
import { GadgetContextProvider } from "../contexts/GadgetContextProvider";
import type { GadgetInfo, GadgetRuntime } from "../contexts/GadgetContext";
import { sendGadgetMessage } from "../lib/gadgetMessage";
import { SettingsSidebar, type SidebarItem } from "./SettingsSidebar";
import { TitleBar } from "../components/TitleBar";
import { GeneralSection } from "./sections/GeneralSection";
import { AppearanceSection } from "./sections/AppearanceSection";
import { FrecencySection } from "./sections/FrecencySection";
import { WebsiteMetadataSection } from "./sections/WebsiteMetadataSection";
import { GadgetsManagementPanel } from "./sections/GadgetsManagementPanel";
import { GadgetSettingsWrapper } from "./GadgetSettingsWrapper";

// =========================================================
// Built-in sidebar sections
// =========================================================

const GENERAL_SECTIONS: SidebarItem[] = [
  { id: "general", label: "General", icon: "heroicons:cog-6-tooth" },
  { id: "plugins", label: "Gadgets", icon: "heroicons:puzzle-piece" },
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
  const gadgetSections = useMemo(() => getGadgetsWithSettings(), []);

  return (
    <div className="relative flex h-screen font-sans antialiased bg-surface text-text-primary">
      {/* Title bar overlay — renders the drag region and window
          controls absolutely on top of the layout below, so
          swapping the platform variant does not affect anything
          else. See ADR 0034. */}
      <TitleBar />

      {/* Sidebar — a floating card inset from all four window
          edges with equal margins. Inner `pt-10` keeps the nav
          content clear of the TitleBar overlay above it (the
          overlay's drag region extends to y=48 from window top;
          card top sits at y=12, so content needs ≥36px of top
          padding to be interactive). */}
      <aside
        className="relative w-[200px] shrink-0 m-3 rounded-xl bg-surface-sidebar overflow-y-auto scrollbar-muted"
        style={{ boxShadow: "var(--sidebar-shadow)" }}
      >
        {/* Fade mask — covers only the content area (not the
            scrollbar) so sidebar items fade out beneath the
            traffic-light controls. */}
        <div
          className="sticky top-0 z-10 h-10 -mb-10 pointer-events-none"
          style={{
            background: "linear-gradient(to bottom, var(--color-surface-sidebar) 85%, transparent)",
          }}
        />
        <div className="pt-8">
          <SettingsSidebar
            generalItems={GENERAL_SECTIONS}
            customizationItems={CUSTOMIZATION_SECTIONS}
            pluginItems={gadgetSections}
            activeId={activeSection}
            onSelect={setActiveSection}
          />
        </div>
      </aside>

      {/* Content area */}
      <main className="flex-1 overflow-y-auto scrollbar-muted pt-12 pl-6 pr-8 pb-6">
        <SectionContent activeSection={activeSection} gadgetSections={gadgetSections} />
      </main>
    </div>
  );
}

// =========================================================
// Section content router
// =========================================================

function SectionContent({
  activeSection,
  gadgetSections,
}: {
  activeSection: string;
  gadgetSections: ReturnType<typeof getGadgetsWithSettings>;
}) {
  // Built-in sections
  if (activeSection === "general") {
    return <GeneralSection />;
  }
  if (activeSection === "plugins") {
    return <GadgetsManagementPanel />;
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

  // Plugin sections — wrapped in GadgetSettingsWrapper for the
  // standardized header + enable/disable toggle. Custom settings
  // components (if any) are rendered as children.
  const plugin = gadgetSections.find((p) => p.id === activeSection);
  if (!plugin) {
    return null;
  }

  // Stable reference — registry returns the same component instance per ID.
  const CustomSettings = getGadgetSettingsComponent(plugin.id);

  return (
    <Suspense fallback={<div className="text-text-muted text-sm">Loading settings…</div>}>
      <PluginSectionContent plugin={plugin} CustomSettings={CustomSettings} />
    </Suspense>
  );
}

// =========================================================
// Plugin section content
//
// Renders the GadgetSettingsWrapper (header + enable/disable)
// with an optional custom settings component as children.
// Memoizes the scoped hook factory per plugin ID.
// =========================================================

function PluginSectionContent({
  plugin,
  CustomSettings,
}: {
  plugin: ReturnType<typeof getGadgetsWithSettings>[number];
  CustomSettings?: ComponentType<GadgetSettingsProps>;
}) {
  // The plugin's enabled flag flows through GadgetContext so
  // setting components can read it via useGadgetInfo() and
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
        sendGadgetMessage<TPayload, TResult, TStream>(plugin.id, method, payload, onMessage),
    [plugin.id],
  );

  const info = useMemo<GadgetInfo>(() => ({ id: plugin.id, enabled }), [plugin.id, enabled]);
  const runtime = useMemo<GadgetRuntime>(() => ({ sendMessage, logger }), [sendMessage, logger]);

  return (
    <GadgetSettingsWrapper
      pluginId={plugin.id}
      icon={plugin.icon ?? "heroicons:puzzle-piece"}
      name={plugin.label}
      description={plugin.description ?? ""}
    >
      {CustomSettings && (
        <GadgetContextProvider info={info} runtime={runtime}>
          <CustomSettings />
        </GadgetContextProvider>
      )}
    </GadgetSettingsWrapper>
  );
}
