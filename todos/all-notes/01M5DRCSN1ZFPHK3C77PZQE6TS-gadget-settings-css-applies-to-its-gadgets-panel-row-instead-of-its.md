---
title: "Gadget settings CSS applies to its Gadgets panel row instead of its settings page"
kind: bug
component: frontend
origin: review
tags: [ux]
---
# Gadget settings CSS applies to its Gadgets panel row instead of its settings page

A WASM gadget's `settings-css` is injected wrapped in `@scope ([data-gadget="<id>"])` (`injectGadgetCss` in `src/lib/gadgetCss.ts`, called from `src/gadgets/wasmPluginLoader.ts` for the settings webview). In the Settings window the only element carrying `data-gadget` is the gadget's row in the Gadgets panel (`PluginRowView` in `src/settings/sections/GadgetsManagementPanel.tsx`, added with commit 8870cc1e); `GadgetSettingsWrapper` and `SettingsPanel` set none. So the gadget's settings styles match its panel row and never its own settings page. Found on 2026-10-21 in the plan review of the failed-gadget note indent, confirmed in the code, not checked in the running app.

## Impact

The settings pages of gadgets that ship `settings-css` (calculator, zerotier, bangs, the template) render without their styles, and the styles can leak into the Gadgets panel row.

## Suggested fix

Put `data-gadget={gadgetId}` on the element that wraps the gadget's settings content in `GadgetSettingsWrapper`, and decide whether the panel row needs its own attribute name so gadget CSS cannot reach host UI. Test that a scoped rule matches the settings page and not the panel row.
