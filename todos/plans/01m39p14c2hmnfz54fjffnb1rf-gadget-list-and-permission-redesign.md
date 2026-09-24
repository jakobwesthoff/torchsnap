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

| Step | Commit |
|---|---|
| B. Pending overview with kind, name, description and versions (`pending_gadget_changes`) | `bbda2b3` |
| C. Pending states in the list, restart bar, versions; result banner removed | `bbda2b3` |
| D. Grouped permission component on the cards and in the review | `135334c` |
| F. "Restart now" calls `restart_to_apply_gadget_changes`, which leaves `.reopen-gadget-settings` in the app data dir; `setup` consumes it, opens Settings, and the frontend starts on Gadgets via `take_settings_start_section` | `0d39de3` |
| E. ADR 51 and CHANGELOG | `e9cb0e1` |
| E. torchsnap-docs Settings page | docs `ab3f893` |

Open:

- New Settings → Gadgets screenshots, which the maintainer provides.
- The maintainer's next review round of the rebuilt unsigned release
  bundle.

## Working rules

Same as the install pipeline plan: tasks per step, tests first with an
observed failure, `just fullcycle` before every commit, one commit per
step, MPL headers on new files, CHANGELOG entries in the commit that
makes a change visible.
