# `useSetting` returns the previous key's value when its `key` argument changes

**Kind:** bug
**Severity:** medium
**Area:** src/hooks/useSetting.ts

## Problem

`useSetting` reads the store only in the `useState` initializer,
which runs once per component instance
(`src/hooks/useSetting.ts:26-28`):

```ts
const [value, setValueState] = useState<T>(() => getSettingSync<T>(key));
```

When the `key` argument changes on a later render, the
subscription effect re-subscribes for the new key
(`useSetting.ts:34-43`), but the *state* still holds the old key's
value. Nothing re-reads `getSettingSync(key)` for the new key, so
the hook returns the stale value until some future
`settings-changed` event happens to fire for the new key.

Concrete broken call site: `useOptionalGadgetEnabled` in the
launcher (`src/launcher/Launcher.tsx:132-137`):

```ts
const [value] = useSetting<boolean>(
  gadgetId ? `enabled.${gadgetId}` : "__internal__.no-active-gadget",
);
```

The launcher never remounts; the key flips between the sentinel and
`enabled.<id>` whenever a gadget custom/inline view activates or
changes. Sequence: launcher mounts with no active gadget → state
initializes to `getSettingSync("__internal__.no-active-gadget")`
(`undefined`) → user activates the clipboard view → key becomes
`enabled.clipboard-manager`, but `value` remains `undefined`. The
`GadgetInfo.enabled` passed into every gadget view
(`Launcher.tsx:484-490`) is therefore `undefined` (falsy) even for
an enabled gadget, until the user toggles that flag in settings.
The context documentation promises the opposite: "The `enabled`
flag ... is read reactively ... so gadget components see disable
toggles immediately" (`Launcher.tsx:479-482`).

Other dynamic-key call sites are exposed to the same hazard
whenever they re-render with a different id instead of remounting:
`src/settings/SettingsPanel.tsx:155`,
`src/settings/GadgetSettingsWrapper.tsx:41`,
`src/settings/sections/GadgetsManagementPanel.tsx:215`, and
`useGadgetSetting` (`src/contexts/useGadgetSetting.ts`).

Second concrete broken call site (settings window): `SectionContent`
renders `<GadgetSectionContent gadget={gadget} ...>` for the active
gadget section without a `key` prop
(`src/settings/SettingsPanel.tsx:133-137`). Switching directly from
gadget A's settings page to gadget B's reconciles the same component
in place, so both `GadgetSectionContent`'s
`useSetting(\`enabled.\${gadget.id}\`)` (`SettingsPanel.tsx:155`) and
`GadgetSettingsWrapper`'s (`GadgetSettingsWrapper.tsx:41`) keep A's
value while labeled as B: the "Enable <name>" toggle shows the wrong
state, and a user "correcting" it writes the flipped value to B's
real key. (`PluginRowView` is safe — rendered with `key={row.id}`,
`GadgetsManagementPanel.tsx:195`.) Independent of the hook fix,
`<GadgetSectionContent key={gadget.id} ...>` removes this
manifestation locally.

## Impact

`useGadgetInfo().enabled` is wrong (undefined) inside gadget views
in the common path; any settings component that switches gadget id
without a remount shows the previous gadget's enabled/setting
values.

## Suggested fix

Add the prev-key-during-render pattern the codebase already uses
elsewhere (e.g. `Launcher.tsx:295-301`):

```ts
const [prevKey, setPrevKey] = useState(key);
if (prevKey !== key) {
  setPrevKey(key);
  setValueState(getSettingSync<T>(key));
}
```

Add a test/story: mount a component with key A, flip to key B,
assert the value tracks B synchronously.
