---
kind: plan
status: in-progress
plan: todos/plans/01m399736658afgk6xv4ab68wk-open-gadget-archives-from-outside-the-app.md
---

# Gadget list, pending changes and permission presentation

Follow-up to the install pipeline plan, from the maintainer's review of
the unsigned release build on 2026-09-24. Every item below was asked for
or decided by the maintainer in that review.

## Already done (branch `gadget-file-association`)

| Feedback | Change | Commit |
|---|---|---|
| Switches not aligned; the ZeroTier one is shifted by the Uninstall button | Fixed-width trailing slot on every gadget row | `cedff16` |
| Permissions too obtrusive | One collapsed line per gadget that expands on click; built-in gadgets show a disabled "Permissions aren't listed for built-in gadgets" | `cedff16` |
| Yellow and grey bars unexplained | Colour bars removed; broad grants carry a "Broad access" label with an explanatory tooltip | `cedff16` |
| Restart prompt still shown after everything was undone | Undo reports whether anything is still pending for the gadget; "No restart needed." instead of the button when nothing is | `3fa6628` |
| Undo button looks poor | Bordered small button | `3fa6628` |
| Uninstall could be pressed twice, the second time showing an error | Pending uninstalls show a disabled "Removed on restart" instead of Uninstall (`pending_gadget_changes` command) | `10f1950` |
| "System" should read "Bundled", as in the docs | Badge, tooltip and rejection messages say bundled; docs updated | `10f1950`, docs `e99f1e8` |
| Uninstall should be undoable | Uninstall keeps the startup archive as `.<id>.torchsnap.prev`; undo restores it and removes the marker. A reinstall after an uninstall is its own state (`Reinstalled`), so undoing it returns to "uninstalled" | `9c3d4ac` |

## Decisions for the remaining work

Decided in the question round of 2026-09-24:

- **Pending changes live in the gadget list**, not in a result banner.
  Rows with a pending change are dimmed and say what happens on
  restart; Undo sits in the row's trailing slot. A fresh install
  appears in the list before the restart. One persistent bar above the
  list counts the pending changes and offers "Restart now". Because the
  state comes from the backend, it survives closing and reopening
  Settings. The result banner is removed.
- **Uninstall is undoable** (done, see above).
- **Permissions are grouped by what they touch**: Runs programs,
  Network, Files, Opens links, Clipboard, Own data, each with an icon.
  The collapsed line on a card names the groups instead of counting
  items. Broad groups carry the labelled "Broad access" tag. Paths are
  shortened (`~` for the home directory), and patterns for other
  operating systems are collapsed behind "+N for other systems". In a
  replace review, New and Removed markers sit inside each group, with a
  one-line "Adds … · Drops …" summary on top. The same component is used
  on the cards and in the install and replace review.
- **Versions appear muted after the gadget name**, "1.2.0 → 2.0.0" for a
  pending replace. Built-in gadgets have no manifest version and show
  none.
- **After "Restart now", Torchsnap opens Settings on the Gadgets section
  on the next start**, so the user sees what the restart applied.

## Steps

**B. Pending overview with names and versions (backend).** Implemented
and tested, not committed: `pending_gadget_changes` returns, per gadget,
the kind (`installed`, `replaced`, `uninstalled`, `reinstalled`), name,
description, the version on disk after restart and the version running
until then. It lands together with C, because the current panel expects
the old string values.

**C. Pending states in the list, restart bar, versions (frontend).**
- Merge registered gadgets with pending installs that are not
  registered yet.
- Row states: "Installs on restart", "Updates to X on restart",
  "Reinstalls on restart", "Removed on restart"; dimmed row; Undo in the
  trailing slot.
- Restart bar above the list with the number of pending changes.
- Version after the name.
- Remove `InstallResultBanner` and the batch results in
  `useInstallQueue`; confirm, undo and uninstall refresh the pending
  state instead.
- Tests first for each state, the bar, versions, and persistence across
  remounts.

**D. Grouped permission component.**
- Group mapping, path shortening and other-OS detection as pure,
  tested functions.
- One component with a collapsed line (group names, broad-access label)
  and an expanded grouped view; diff mode with New/Removed markers and
  the Adds/Drops summary.
- Used on the gadget cards and in `InstallReviewModal`.

**F. Reopen Settings on Gadgets after "Restart now".**
- "Restart now" calls a backend command that records a one-shot marker
  and relaunches.
- `setup` consumes the marker and opens Settings; the settings frontend
  starts on the Gadgets section.
- Tests for the marker round trip and the start section.

**E. Records and docs.**
- ADR 51: uninstall keeps the startup archive and is undoable; pending
  state is shown in the list instead of a result banner.
- CHANGELOG entries for the user-visible changes.
- torchsnap-docs Settings page: list states, restart bar, grouped
  permissions.
- New Settings → Gadgets screenshots, which the maintainer provides.

**Then:** rebuild the unsigned release bundle (`just build --release`,
`just verify-bundle release`) for the maintainer's next review round.

## Working rules

Same as the install pipeline plan: tasks per step, tests first with an
observed failure, `just fullcycle` before every commit, one commit per
step, MPL headers on new files, CHANGELOG entries in the commit that
makes a change visible.
