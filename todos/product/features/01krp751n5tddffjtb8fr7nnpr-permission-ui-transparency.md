---
kind: feature
status: open
tags: [security, ux]
plan: todos/plans/01m399736658afgk6xv4ab68wk-open-gadget-archives-from-outside-the-app.md
---

# Show gadget permissions in UI

Two features needed:

## 1. Settings page: permission display (done)

Done in plan step 13: the gadget cards in Settings → Gadgets list each
WASM gadget's permissions through `PermissionSummary`.

Each gadget's settings page should automatically show its declared permissions
in a structured, readable format alongside the enable toggle. Users should be
able to see at a glance what a gadget can access (clipboard, HTTP origins,
filesystem paths, command binaries, etc.) without reading the raw manifest.

This is particularly important for third-party gadgets where the user did not
write the manifest themselves.

## 2. Install flow: permission review

When a user installs a `.torchsnap` archive (drag-and-drop or future install
UI), the install flow should parse the manifest and present the requested
permissions before completing the installation. The user should be able to
review and accept or reject. The presentation should be clear and non-technical
where possible (e.g. "Can read files matching ~/.config/myapp/*" rather than
the raw glob).

## Implementation notes

The manifest is already parsed at install time (`install_gadget_archive` in
`gadget_install.rs`). The permissions struct is available. The work is
primarily frontend: rendering the permission data in the settings panel and
building the install confirmation dialog.
