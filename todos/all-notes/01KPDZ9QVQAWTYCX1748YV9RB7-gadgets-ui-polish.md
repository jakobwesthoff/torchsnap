---
kind: improvement
status: open
area: [src/settings/SettingsSidebar.tsx, src/settings/SettingsPanel.tsx]
---

# Mark gadgets with pending changes in the Settings sidebar

The Gadgets list in Settings shows installs, updates and uninstalls
that wait for a restart (dimmed row, a note such as "Removed on
restart", Undo), built from the `pending_gadget_changes` command
(`src/settings/install/usePendingChanges.ts`). The sidebar does not
reflect this: a gadget that is removed on restart still has its
settings entry under "Gadgets" looking like any other.

Mute the sidebar entry of a gadget with a pending uninstall, so its
settings read as inert until the restart. `SidebarButton` has no
muted or disabled variant yet; add one (for example
`text-text-tertiary`) and pass the pending state from `SettingsPanel`,
which would need the pending changes as well. Whether the entry should
still open the gadget's settings is open.

Hot lifecycle (no restart at all) is tracked separately in
`todos/gadget-host/wasm/01kpdsvj5at6agxst1jva5eeva-gadget-hot-lifecycle.md`.
