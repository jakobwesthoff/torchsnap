---
kind: refactor
status: open
area: [gadgets/gadget-sdk/wit/torchsnap-gadget.wit, gadgets/gadget-sdk/src/lib.rs, gadgets/gadget-sdk/src/data.rs, src-tauri/src/gadgets/mod.rs, src-tauri/src/commands/types.rs, src-tauri/src/commands/mod.rs, src-tauri/src/gadget_host.rs, src-tauri/src/entry_store.rs, src-tauri/src/wasm/bindings.rs, src-tauri/src/wasm/bridge.rs, src/types.ts, src/launcher/Launcher.tsx, src/launcher/ResultRow.tsx, src/launcher/hooks/useKeyboardNavigation.ts, packages/gadget-sdk/src/types/data.ts, packages/gadget-sdk/src/shims/hooks.ts]
tags: [api-design, wasm, sdk]
plan: todos/plans/01m3cjnk9q29z0s23xyysqcm19-entry-action-commands-implementation.md
depends-on: [todos/backend/search/01kwg1ph0qcdqtara5jcw7abym-concurrent-searches-corrupt-entry-store.md]
---

# Entry action commands and fixed slots

Implement ADR 55
(`docs/adr/0055-dispatch-entry-actions-through-gadget-commands-in-fixed-slots.md`).
The ADR holds the decisions: commands instead of `ActionId` and
`data`, the six slots and their keys, the frontend owning keys and
default labels, the typed `Search` traits in SDK and host, and the
rule for views. This todo holds the migration work.

Implement typed messaging in the same refactor:
`todos/gadget-host/sdk/01m3cg9f3dnqqvgf4pr1rwtt7j-typed-messaging-request-enum.md`.

## Work

**WIT** (`gadgets/gadget-sdk/wit/torchsnap-gadget.wit`)

- Replace `variant action-id` and `record action` with the `action` and
  `entry-actions` records from the ADR. Use `entry-actions` in
  `catalog-entry` and `scored-entry`.
- Remove `scored-entry.data`. Keep `view-response.data`.
- `execute: func(command: string) -> result<post-action, string>`.
- The doc comment on `command` says: opaque, returned unchanged, never
  inspected by the host. It does not mention JSON.
- Bump the package version from `torchsnap:gadget@0.1.0` to
  `torchsnap:gadget@0.2.0`, and both SDKs to `0.2.0` with it
  (ADR 56): `gadgets/gadget-sdk/Cargo.toml` and
  `packages/gadget-sdk/package.json`. Other places
  that name the version: `docs/Gadget-Architecture/01-overview.md:4`
  and the "Documentation version" note in torchsnap-docs
  `development/interfaces/index.mdx:215`.

**Host**

- Split `entries()`, `search()` and `execute()` out of `Gadget`
  (`src-tauri/src/gadgets/mod.rs:74`) into `ErasedSearch` and the typed
  `Search` with a blanket impl. `Gadget: ErasedSearch`.
- Host entry types carry the slot record with opaque commands. `Action`
  loses `keybinding`, and `default_keybinding_for`
  (`src-tauri/src/wasm/bindings.rs:157`) goes away.
- Serialize actions to the frontend as a `{ slot, label }` list in slot
  table order, without commands.
- `search_execute` (`src-tauri/src/commands/mod.rs:48`) takes
  `(source, entry_id, slot)`. `GadgetHost::execute` takes the command
  from the stored entry's slot. Empty slot: log and return `Nothing`.
- The WASM bridge implements `ErasedSearch` directly and passes command
  strings through.

**Frontend**

- One table from slot to key and to default label. Keys: `primary`
  Enter and click, `secondary` Cmd+Enter, `copy` Cmd+C, `reveal`
  Cmd+Shift+R, `delete` Cmd+Backspace, `open-settings` Cmd+,. Default
  labels, the same English text on every platform: "Copy", "Reveal in
  Finder", "Delete", "Open settings".
- A `primary` or `secondary` action without a label shows the entry
  title.
- Replace every "first action is primary" lookup with the `primary`
  slot: footer (`src/launcher/Launcher.tsx:38`), `executeEntry`
  (`Launcher.tsx:591`), row click (`src/launcher/ResultRow.tsx:111`),
  Enter (`src/launcher/hooks/useKeyboardNavigation.ts:162`). Secondary
  bindings (`useKeyboardNavigation.ts:182`) read keys from the slot
  table.
- `LauncherActions.onExecute(entryId, slot)` and a new
  `openSettings()` (`packages/gadget-sdk/src/shims/hooks.ts:124`).
  `openSettings()` needs a Tauri command that opens the settings window
  on the calling view's gadget section, as
  `show_settings_window_at(app, source)` does for the post-action.
- `ActionId` in `src/types.ts:23` and
  `packages/gadget-sdk/src/types/data.ts:21` becomes a slot string
  union.

**Rust SDK**

- `Search` trait, blanket `SearchGuest` impl, SDK-owned generic entry
  types, and `Actions<C>` with public `Option` fields, `Default`,
  builder and `iter()`, as in the ADR.
- `SearchGuest` leaves the prelude and stays at the crate root. The
  `data` module leaves the prelude.

## Gadget migration

| Gadget | Slots and commands |
|---|---|
| awake | `primary`: `Start {..}` or `Stop` (today's `Op`). The error entry stays without actions. |
| bangs | `primary`: open URL. `copy`: copy URL. |
| open-url | `primary`: open URL. `copy`: copy URL. |
| zerotier | `primary`: toggle(id), join or leave decided from live state at execute time. `copy`: copy id. `delete`: forget(id). Failure entries: `primary` labeled "Open settings" with a command that returns the `open-settings` post-action, so Enter keeps working. They do not fill the `open-settings` slot. `parse_entry_id` goes away. |
| calculator | History rows: `primary` and `copy` with a copy command carrying expression and result. The current result goes through a message plus `dismiss()` (see the calculator bug todo below). |
| emoji-picker | `copy`: copy emoji. The grid calls `onExecute(entry.id, "copy")`. |
| hello-world | Petnames: `primary` labeled "Copy" and `copy`, same command, since Enter copies today. `execute()` writes the name to the clipboard (today it only logs), which needs `clipboard = true` under `[permissions]` in its manifest. |
| template | demo command in `primary`. |
| app_launcher | `primary`: open(path). `secondary`: reveal(path), label "Reveal in Finder", keeps Cmd+Enter. |
| system_preferences | `primary`: open pane(id). |
| commands | `primary`: quit, settings or devtools. |
| system_commands | `primary`: run(command id). |
| clipboard | `primary`: show history. The history view keeps using messages. |

`execute()` in hello-world and template logs `entry.id`. Put what the
log needs into the command.

## Renaming `open` to `primary`

These places carry today's `open` meaning and all move to the
`primary` slot: the WIT `action-id` variant, host `ActionId::Open`
(`src-tauri/src/commands/types.rs`, `src-tauri/src/wasm/bindings.rs`),
the WASM gadgets awake, bangs, hello-world, open-url, template and
zerotier, the native gadgets app_launcher, clipboard, commands,
system_preferences and the three `system_commands/macos_commands`
files, the TS `{ type: "open" }` types, and the frontend lookups listed
above.

Keep these, they mean something else: the `open-settings` slot and
post-action, the `opener` capability (`open_url`, `open_path`), the
open-url gadget's name, and gadget labels such as "Open" or "Open in
Browser".

## Checks during implementation

- The blanket impl was tested only on the host target with stand-in
  traits. Confirm it compiles with the `wit_bindgen` `export!` for
  `wasm32-wasip2` before converting the gadgets.
- `entries()` runs on every search without a prefix, and the WASM
  bridge does not cache it (`src-tauri/src/gadget_host.rs:932`,
  `src-tauri/src/wasm/bridge.rs:651`). Every catalog entry's commands
  are encoded each time, which includes one command per installed app
  in app_launcher. Measure it.
- Check what a gadget built against the old WIT version reports when
  loaded after the bump.

## Docs

torchsnap-docs describes `ActionId`, `custom(string)` and `data`:
`development/search.mdx` (the `ActionId` examples, and the "Custom
actions" section at line 381), `development/interfaces/imports.mdx:45`,
`development/interfaces/exports.mdx` and
`development/your-first-gadget.mdx`. Update them with the code.

## Related

- `todos/gadgets/calculator/01m3cfkn09sjm61eemfe83vndq-calculator-copy-on-enter-does-nothing.md`:
  fix it before this refactor. Its fix already follows the ADR's rule
  for views.
- `todos/backend/search/01kwg1ph0qcdqtara5jcw7abym-concurrent-searches-corrupt-entry-store.md`
  (in `depends-on`): overlapping searches can leave stale entries in
  the store. With commands, a stale entry runs a stale command. Land
  that fix first.
- `todos/product/features/01kmh12n6r0mq94rwav32eczdn-keybind-system.md`:
  gadget-defined bindings, deferred by the ADR.
