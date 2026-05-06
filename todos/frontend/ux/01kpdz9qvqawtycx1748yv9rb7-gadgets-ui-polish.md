# Gadgets UI — UX Polish

Three related issues in `src/settings/sections/GadgetsManagementPanel.tsx`
and `src/settings/SettingsSidebar.tsx` that need to be addressed together.

---

## 1. Fixed-width trailing column — prevent toggle jitter

**File:** `GadgetsManagementPanel.tsx` → `GadgetRowView`

The trailing slot in each `GadgetRowView` row renders either a `<button>Uninstall</button>`
or a `<SourceBadge>`, whichever applies to the gadget's `sourceKind`. These
two elements have different intrinsic widths, so the `<Switch>` to their left
shifts horizontally across rows.

**Fix:** Give the trailing slot a fixed width (e.g. `w-24` or a measured
`min-w-[...]`) so the column is the same for every row regardless of which
element is rendered. The `<Switch>` will then stay pinned at a consistent
horizontal position.

---

## 2. "Needs restart" — make the state persistent and prominent

**File:** `GadgetsManagementPanel.tsx` → `BannerView`, `GadgetRowView`, `GadgetsManagementPanel`

### 2a. Remove the Dismiss button from restart banners

`BannerView` currently always renders a `Dismiss` button. For success banners
that carry `requiresRestart: true` this makes no sense — the restart
requirement does not go away if the user dismisses it. The `Dismiss` button
must be removed for restart banners. Errors can still be dismissed.

Beyond removing Dismiss, the banner itself should be more prominent: consider
a sticky/persistent callout rather than the current inline `rounded-lg` strip,
so it stays visible even when the user scrolls.

### 2b. Keep installed/uninstalled gadgets in the list, marked "pending restart"

Currently the `rows` array in `GadgetsManagementPanel` is derived purely from
`sourceKinds` (backend snapshot) crossed with `gadgetMetadata` (frontend
registry). An uninstalled gadget disappears from the list immediately because
the backend no longer reports it. An installed gadget only appears after
`sourceKinds` is refreshed (which never happens in the same session).

Instead, maintain a local `pendingRestart` set in component state that tracks
which gadget IDs have been installed or uninstalled during this session. Rows
derived from that set should:

- Remain visible in the "Installed Gadgets" list with a clear "Needs restart"
  badge or label in place of their normal source badge / uninstall button.
- Appear visually deactivated: reduced opacity or greyed-out text, so the user
  understands they are inert for this session.
- Be present — but also visually deactivated — in the sidebar under the
  "Gadgets" group in `SettingsSidebar`. This requires either propagating the
  pending-restart set up to `SettingsPanel` / `SettingsSidebar` or providing a
  separate mechanism (store entry, context) to mark sidebar items as disabled.

### 2c. Lock the toggle off after uninstall

After `handleUninstall` completes successfully, the uninstalled gadget's row
must:

- Have its `<Switch>` locked in the off position and marked `disabled` for the
  rest of the session.
- Not allow the user to flip it back on (the enable setting write should be
  gated on "not pending uninstall").

The cleanest approach is to add the gadget ID to the `pendingRestart` set
(from 2b) and pass an `isLockedOff` flag into `GadgetRowView`, which then
passes `disabled` to `<Switch>` when true.

---

## Notes

- The `<Switch>` component already supports a `disabled` prop (renders with
  reduced opacity) — leverage it.
- For the sidebar, `SidebarButton` currently has no disabled/muted variant.
  A `disabled` or `muted` prop will need to be added that renders the item
  with `text-text-tertiary` and disables click.
- Hot-lifecycle removal (no restart needed at all) is tracked separately in
  `todos/wasm/…-gadget-hot-lifecycle.md` — do not conflate the two.
