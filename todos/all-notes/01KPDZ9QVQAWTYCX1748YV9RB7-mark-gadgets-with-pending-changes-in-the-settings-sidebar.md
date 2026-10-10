---
title: "Mark gadgets with pending changes in the Settings sidebar"
kind: improvement
component: frontend
tags: [ux]
---
# Mark gadgets with pending changes in the Settings sidebar

The Gadgets list in Settings shows installs, updates and uninstalls that wait for a restart. Those rows are dimmed, carry a note such as "Removed on restart", and offer Undo. They are built from the `pending_gadget_changes` command (`src/settings/install/usePendingChanges.ts`). The sidebar does not reflect this: a gadget that is removed on restart still has its settings entry under "Gadgets", looking like any other.

## Proposal

Mute the sidebar entry of a gadget with a pending uninstall, so its settings read as inert until the restart. `SidebarButton` has no muted or disabled variant yet, so add one (for example `text-text-tertiary`). `SettingsPanel` passes the pending state to it, which needs the pending changes as well.

## Open questions

- Whether the entry should still open the gadget's settings.

## Relations

- Relates to: [Install, uninstall and reload gadgets without an app restart](01KPDSVJ5AT6AGXST1JVA5EEVA-install-uninstall-and-reload-gadgets-without-an-app-restart.md), the same pending state without a restart, tracked separately.
