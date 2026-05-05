# Phase 3 — Internal Code Rename

## Summary

Phase 3 renames Rust types, modules, and source files inside `src-tauri/`, plus internal TypeScript files and symbols inside `src/`, from the `Plugin*` family to the `Gadget*` family. It also flips the host-only URI scheme `torchsnap-plugin://` to `torchsnap-gadget://` on both Rust and TS sides. The phase explicitly excludes every wire-/public-API surface (Tauri command names, Tauri event names + payload keys, settings-key prefix `plugins.`, manifest permission-var tokens `${plugin-data}` and `${plugin-archive}`, `define_plugin!`, WIT, SDK crate/package names, `manifest.toml` `[plugin]` section, `bundled.toml` `plugins` key) — those are P4 — and excludes every directory rename and storage path — those are P5. The result of P3 is a host whose internal symbol vocabulary has flipped to `Gadget*` while every wire shape, file path it serves, and storage layout still uses `plugin*`.

## Predecessors

- **P1 (UI strings)** is independent of P3.
- **P2 (docs/ADR)** is independent of P3 (P2's prose can land before or after; P3 only updates rustdoc/JSDoc that lives inside renamed source files).
- The shared **inventory.md naming map** must be locked in. P3 is the first phase that reaches the symbol surface, so all decisions must be resolved before the first commit of this phase lands.

## Successors

- **P4 (wire surface)** must NOT start before P3 lands. P4 will assume that `PluginHost` has already become `GadgetHost`, so P4's edits to `wasm_plugins`/`plugin_sources` Tauri command names, `pluginId` event payload, and `plugins.` settings prefix run on top of the renamed Rust types.
- **P5 (repo layout / storage paths)** must run after P3 to keep `git mv` operations atomic and avoid path/import churn in the same commit as symbol churn.

## Scope

### In

Rust (`src-tauri/src/`):

- File renames via `git mv`:
  - `src-tauri/src/plugin_host.rs` → `gadget_host.rs`
  - `src-tauri/src/plugin_install.rs` → `gadget_install.rs`
  - `src-tauri/src/frecency/plugin_frecency.rs` → `gadget_frecency.rs`
- Directory rename via `git mv`:
  - `src-tauri/src/plugins/` → `src-tauri/src/gadgets/` (the host built-ins module — distinct from the WASM plugin workspace at `plugins/` which is P5)
- Type renames per the inventory.md naming map, plus these inventory-omitted host-internal types: `PluginShortcut` (`src-tauri/src/plugins/mod.rs:43`), the Rust `PluginContext` struct (`src-tauri/src/plugins/mod.rs:66`, distinct from the TS context object), `PluginMeta` (`src-tauri/src/wasm/manifest/mod.rs:105`), `PluginIcon` (`manifest/mod.rs:134`), `PluginResponse` (`src-tauri/src/commands/types.rs:224` — not serialized), the Rust `PluginViewRef` type (`src-tauri/src/commands/types.rs:252` — type rename only; the wire-bound `plugin_id` field stays for P4), `PluginSettings` and `SettingsInit` (`src-tauri/src/settings/mod.rs:131,45` — `PluginSettings` renames to `GadgetSettings` per inventory; `SettingsInit` keeps its name but its rustdoc/comments referring to plugins as a domain, including the doctest `from_store(&store, "plugins.clipboard-manager.")` on lines 38-43, stay until P4 owns the settings-key prefix), `ActivatePluginPayload` (`src-tauri/src/plugin_host.rs:76`), and the `plugin_id` field on `RegisteredShortcut` (`src-tauri/src/plugin_host.rs:67`, host-internal).
- URI scheme registration and consumption: `torchsnap-plugin://` → `torchsnap-gadget://` on Rust (`wasm/protocol.rs`, `wasm/bindings.rs`) and TS (`src/lib/pluginCss.ts`, `src/plugins/wasmPluginLoader.ts`, `src/components/Icon.tsx` rustdoc/JSDoc, `wasm/manifest/frontend.rs:48` doc comment). The scheme is host-internal between host and host-rendered code; no plugin-author doc in `docs/` cites it as an author-callable contract.
- HTML attributes `data-plugin` and `data-plugin-css` (host-side container in `src/launcher/Launcher.tsx`, host-side CSS injection in `src/lib/pluginCss.ts`).

TypeScript (`src/`):

- File renames via `git mv`:
  - `src/contexts/PluginContext.tsx` → `GadgetContext.tsx`
  - `src/contexts/PluginContextProvider.tsx` → `GadgetContextProvider.tsx`
  - `src/contexts/usePluginInfo.ts` → `useGadgetInfo.ts`
  - `src/contexts/usePluginRuntime.ts` → `useGadgetRuntime.ts`
  - `src/contexts/usePluginSetting.ts` → `useGadgetSetting.ts`
  - `src/hooks/usePluginStream.ts` → `useGadgetStream.ts`
  - `src/lib/sdk.ts` (no path rename — symbol rename only, file is generic)
  - `src/lib/pluginCss.ts` → `gadgetCss.ts`
  - `src/lib/pluginMessage.ts` → `gadgetMessage.ts`
  - `src/lib/pluginComponent.tsx` → `gadgetComponent.tsx`
  - `src/devtools/console/pluginColors.ts` → `gadgetColors.ts`
  - `src/settings/sections/PluginsManagementPanel.tsx` → `GadgetsManagementPanel.tsx`
  - `src/settings/PluginSettingsWrapper.tsx` → `GadgetSettingsWrapper.tsx`
- Inventory-omitted internal TS symbols: `src/plugins/registry.ts` and `src/plugins/wasmPluginLoader.ts` (`PluginRegistryEntry` → `GadgetRegistryEntry`; `registerPlugin`/`unregisterPlugin`/`getPluginView`/`getPluginInlineView`/`getPluginSettingsComponent`/`getPluginsWithSettings` → `register*Gadget*` family; `registerWasmPlugin`/`registerAllWasmPlugins` → `registerWasmGadget`/`registerAllWasmGadgets`). All consumer updates (`Launcher.tsx`, `SettingsPanel.tsx`, the renamed `GadgetsManagementPanel.tsx`, `launcher/main.tsx`, `settings/main.tsx`).
- Every internal occurrence of the JS identifier `pluginId` (locals, parameters, destructure names) renames to `gadgetId`, including consumers in `Launcher.tsx`, `gadgetCss.ts`, `gadgetMessage.ts`, `gadgetColors.ts`, `TreeLogList.tsx`, `ConsoleToolbar.tsx`, `LogItemRow.tsx`. Wire-bound camelCase keys in serialized payloads stay for P4.
- Symbol renames per inventory.md naming map.
- Update `src/contexts/index.ts` barrel exports.
- Type-sync: rename type aliases in `packages/plugin-sdk/src/shims/hooks.ts` (`PluginInfo` → `GadgetInfo`, `PluginRuntime` → `GadgetRuntime`, `PluginSendMessage` → `GadgetSendMessage`, `usePluginInfo`/`usePluginRuntime`/`usePluginSetting` → `useGadget*`), and matching renames in `packages/plugin-sdk/src/testing/MockPluginContextProvider.tsx`, `testing/index.ts`, `types/plugin.ts`, `types/index.ts`, `types/settings.ts`, `shims/components.ts`, `shims/keybindings.ts`, `shims/utils.ts`, `shims/react.ts`, `testing/setup.ts`. The package's *directory rename* is P5 and the package *npm name* `@torchsnap/plugin-sdk` is P4 — they remain unchanged in P3. P3 only touches the in-shim type aliases that the host's compile-time sync check at `src/contexts/PluginContext.tsx:104-124` (newly: `GadgetContext.tsx`) extends-asserts against. The hooks are exposed on `window.__torchsnap.hooks` — the property names there mirror the host (`useGadgetInfo`, etc.) and must be renamed in lockstep.
- `MockPluginContextProvider` (+ `MockPluginContextProviderProps`) → `MockGadgetContextProvider` (+ Props). File `packages/plugin-sdk/src/testing/MockPluginContextProvider.tsx` → `MockGadgetContextProvider.tsx`.
- Rustdoc / JSDoc inside renamed files updated to refer to the new symbol names. Where doc comments mention `plugin` as a domain concept (not a symbol), keep — that prose stays plugin in P3 and gets reframed in P2.

### Out

- Tauri command function names (`wasm_plugins`, `plugin_sources`, `plugin_message`, `install_plugin_archive`, `uninstall_user_plugin`) — P4.
- Tauri event names (`open-plugin-settings`, `activate-plugin-custom-ui`) and payload keys (`pluginId`) — P4. The Rust *type* `ActivatePluginPayload` and its field `plugin_id: String` are renamed in P3 (host-internal). The serde `rename_all = "camelCase"` keeps the wire key as `pluginId` until P4 changes the payload key explicitly.
- Settings-key prefix `plugins.<id>.<key>` literal in `useGadgetSetting.ts:24`, `gadget_host.rs`, `gadget_install.rs` — P4.
- Source discriminator wire string `{"type":"plugin","id":...}` in `commands/types.rs:312-314, 326`, formatters.ts:44 — P4. `formatters.ts:44`'s comparison to `"plugin"` stays as-is in P3; P4 flips the string on both sides atomically.
- WIT package, world, file, `define_plugin!` macro, SDK crate/package directories and names — P4/P5.
- `manifest.toml` `[plugin]` section name — P4.
- `bundled.toml` `plugins = [...]` key — P4.
- App-data dirs `<app_data>/plugins/`, `<app_data>/plugin-home/<id>/` — P5.
- Workspace dirs `plugins/`, `plugins/plugin-sdk/`, `packages/plugin-sdk/` — P5.
- `target/bundled-plugins/` — P5.
- Test fixture dirs `src-tauri/tests/fixtures/*-plugin/` — P5.
- Just recipe file/recipe renames — P5.
- WIT-generated module path `torchsnap::plugin::types::Host` (used at `wasm/bindings.rs:48`) — P4 (driven by WIT package rename).
- Manifest permission-var tokens `${plugin-data}` AND `${plugin-archive}` — P4 (manifest/wire contract surface). Locations: `permission_vars.rs:46–47`, `manifest/permissions/command.rs:537–538`, `bridge.rs:118,536,539`, `source.rs:113,119,399`, `argv_matcher.rs:352`, `runtime/host/fs.rs:368–369`. P3 leaves the field name `plugin_data:` in `runtime/host/fs.rs:368` alone too — P4 owns the renaming of both struct field name and the `"plugin-data"` literal so the substitution stays consistent.
- The `src/plugins/` directory rename to `src/gadgets/` — P5. Files **inside** `src/plugins/` (`registry.ts`, `wasmPluginLoader.ts`, `types.ts`, `clipboard/ClipboardView.tsx`, `clipboard/ClipboardSettings.tsx`) are renamed/edited in P3 where listed; the directory move itself is P5 because `vite.config.ts:48` has `/src/plugins/` baked into a Rolldown `manualChunks` rule that fits P5's batched config-file edits.

### Borderline calls (resolved)

- `formatters.ts:44` `item.source.type === "plugin"`: comparison stays in P3, flipped atomically with the Rust serializer in P4 (see inventory line 70 — source discriminator wire string).
- URI scheme rename in P3: rustdoc at `src-tauri/src/wasm/manifest/frontend.rs:48` mentions `torchsnap-plugin://` only as an internal serving mechanism for files declared in the manifest; no plugin-author-facing doc in `docs/` cites it as a contract that plugin code calls into directly. Treated as host-internal and renamed in P3.
- SDK shim type-aliases in `packages/plugin-sdk/src/`: in P3 scope. The host's compile-time sync check at `src/contexts/PluginContext.tsx:104-124` (post-rename: `GadgetContext.tsx`) will fail to compile if the host renames without the shim renaming in lockstep. The package's *directory* (`packages/plugin-sdk/`) and *npm name* (`@torchsnap/plugin-sdk`) stay until P4/P5.
- `setup.ts` and other shim helpers (`shims/components.ts`, `shims/keybindings.ts`, `shims/utils.ts`, `shims/react.ts`): rename only the `TorchsnapGlobal.hooks.usePluginInfo`/`usePluginRuntime`/`usePluginSetting` property names (and matching consumer literals). The npm-exported subpath strings (`@torchsnap/plugin-sdk/hooks` etc.) inside runtime error messages stay in P3 — P4 owns the npm name.

## Rust rename plan

### File renames

```
git mv src-tauri/src/plugin_host.rs              src-tauri/src/gadget_host.rs
git mv src-tauri/src/plugin_install.rs           src-tauri/src/gadget_install.rs
git mv src-tauri/src/frecency/plugin_frecency.rs src-tauri/src/frecency/gadget_frecency.rs
git mv src-tauri/src/plugins                     src-tauri/src/gadgets
```

Inside `src-tauri/src/gadgets/` (post-`git mv`), no internal sub-file renames needed (`app_launcher.rs`, `clipboard/`, `commands.rs`, `system_commands/`, `system_preferences.rs` keep their names — they describe what the built-in does, not "plugin"-ness).

### Symbol renames per file

For each file listed below, the symbol token (`Plugin` or `plugin`) is replaced with `Gadget`/`gadget` only on host-internal types and identifiers. Wire-tagged serde fields (`plugin_id` in `PluginViewRef`, the `enabled.<id>` and `plugins.<id>.` settings-prefix literals, the `"plugin"` discriminator string, the `pluginId` payload key) are explicitly preserved for P4.

- `src-tauri/src/gadget_host.rs` (entire file, formerly `plugin_host.rs`):
  - Header rustdoc lines 1-25 and trait references throughout: `PluginHost` → `GadgetHost`.
  - Imports lines 38-48: `PluginResponse, PluginViewRef, PluginFrecency, Plugin, PluginContext, PluginShortcut, PluginSettings, PluginSourceKind` updated to `Gadget*` per inventory.
  - Lines 38-39, 42-44, 46, 48 — every `use crate::*::Plugin*` import path follows.
  - Lines 55-58 (`enum ViewKind`): no rename.
  - Lines 66-71 (`struct RegisteredShortcut`): field `plugin_id: String` → `gadget_id: String`.
  - Lines 73-80 (`struct ActivatePluginPayload`): rename to `ActivateGadgetPayload`. **Field `plugin_id: String` stays** (carries `pluginId` over the wire under serde `rename_all = "camelCase"` — P4).
  - Lines 82-85 (`PluginSlot`): rename to `GadgetSlot`.
  - Line 198, 210, 241, 411, 845: settings-key prefix literals `"plugins.<id>."` stay (P4).
  - Lines 743-745: emit payload key `"pluginId"` stays; emit event name `"open-plugin-settings"` stays (P4).
  - Line 981 emit `"activate-plugin-custom-ui"` stays (P4 — wire event name). Both event names are captured in inventory.md's P4 surface list.
  - Tests block (lines 1010+): `MockPlugin` → `MockGadget`, helper `plugin_slots()` → `gadget_slots()`, local idents `plugin`, `plugins` left alone where they refer to a generic concept; renamed where they refer to the renamed types.

- `src-tauri/src/gadget_install.rs` (formerly `plugin_install.rs`):
  - Header rustdoc lines 1-43: type and module references updated. `<app_data>/plugins/` and `<app_data>/plugin-home/` paths stay (P5). Settings-key wording in lines 38-42 stays (P4).
  - Line 53: `use crate::plugin_host::PluginHost` → `use crate::gadget_host::GadgetHost`.
  - Line 54: `use crate::wasm::source::{ArchiveSource, PluginSource, PluginSourceKind}` → `GadgetSource, GadgetSourceKind` (`ArchiveSource` unchanged).
  - Lines 66-71 `struct InstalledPluginInfo` → `InstalledGadgetInfo` (`#[serde(rename_all = "camelCase")]` retained; struct field names `id`, `name`, `version`, `requires_restart` are unchanged so the wire JSON is byte-identical).
  - Line 77-80 `UninstallResult`: no rename.
  - Tauri command function names `install_plugin_archive`, `uninstall_user_plugin` (called from `lib.rs:599-600`) stay (P4).
  - Internal helper symbols (`PluginHost::install_plugin`, etc.) follow.
  - Lines 255, 306: `plugins.<id>.` literal stays (P4).

- `src-tauri/src/frecency/gadget_frecency.rs` (formerly `plugin_frecency.rs`):
  - Type `PluginFrecency` → `GadgetFrecency`. Rustdoc rewritten to refer to gadgets.
  - `src-tauri/src/frecency/mod.rs:22,25`: `mod plugin_frecency;` → `mod gadget_frecency;`, `pub use plugin_frecency::PluginFrecency;` → `pub use gadget_frecency::GadgetFrecency;`.

- `src-tauri/src/gadgets/mod.rs` (formerly `plugins/mod.rs`):
  - Lines 22-26: `pub mod app_launcher; pub mod clipboard; pub mod commands; pub mod system_commands; pub mod system_preferences;` unchanged.
  - Line 28: `use crate::commands::types::{ActionId, CatalogEntry, PluginResponse, PostAction}` → `GadgetResponse`.
  - Line 29: `use crate::frecency::PluginFrecency` → `GadgetFrecency`.
  - Line 30: `use crate::settings::{PluginSettings, SettingsInit}` → `GadgetSettings, SettingsInit`.
  - Lines 43-56 `struct PluginShortcut` → `GadgetShortcut`.
  - Lines 66-69 `struct PluginContext` → `GadgetContext`. Its fields `settings: PluginSettings` and `frecency: PluginFrecency` follow.
  - Line 112 `pub trait Plugin` → `pub trait Gadget`. All trait-method rustdoc updated to refer to "gadget" instead of "plugin".
  - Default impl method `enable(_app, _ctx: &PluginContext)` → `&GadgetContext`.
  - All built-in impls follow: `app_launcher.rs:142`, `system_preferences.rs:80`, `commands.rs:19`, `clipboard/mod.rs:256`, `system_commands/mod.rs:84`.

- `src-tauri/src/gadgets/app_launcher.rs`:
  - Line 39: `use super::{Plugin, PluginContext}` → `Gadget, GadgetContext`.
  - Lines 45, 53, 142-147: `AppLauncherPlugin` → `AppLauncherGadget`.

- `src-tauri/src/gadgets/system_preferences.rs`:
  - Line 35: `use super::{Plugin, PluginContext}` → `Gadget, GadgetContext`.
  - Lines 37, 43, 80, 85: `SystemPreferencesPlugin` → `SystemPreferencesGadget`.

- `src-tauri/src/gadgets/commands.rs`:
  - Lines 17, 19: `BuiltInCommandsPlugin` → `BuiltInCommandsGadget`.

- `src-tauri/src/gadgets/system_commands/mod.rs`:
  - Lines 16, 69, 72, 76, 84: `SystemCommandsPlugin` → `SystemCommandsGadget`.
  - Line 16: `use crate::plugins::Plugin` → `use crate::gadgets::Gadget`.
  - Sub-files at `system_commands/macos_commands/{appearance,power,utilities}.rs:15-17`: `use crate::plugins::system_commands::SystemCommand` → `use crate::gadgets::system_commands::SystemCommand`.

- `src-tauri/src/gadgets/clipboard/mod.rs`:
  - Line 53: `use super::{Plugin, PluginContext}`.
  - Lines 82, 85, 115, 256, 268, 269, 288: `ClipboardPlugin` → `ClipboardGadget`, `super::PluginShortcut` → `super::GadgetShortcut`.

- `src-tauri/src/wasm/bridge.rs`:
  - Line 30: `use crate::plugins::Plugin` → `use crate::gadgets::Gadget`.
  - Lines 42, 49, 165, 585, 632, 1019, 1082, 1089, 1132, 1176, 1317, 1360, 1395, 1518: `WasmPluginBridge` → `WasmGadgetBridge`.
  - Line 632: `impl Plugin for WasmPluginBridge` → `impl Gadget for WasmGadgetBridge`.
  - Line 649: `crate::plugins::PluginContext` → `crate::gadgets::GadgetContext`.
  - Lines 54, 59, 67, 118, 329, 536, 539, 1055, 1281: comments referring to `PluginState`, `PluginContext` are updated (`PluginState` → `GadgetState`, `PluginContext` → `GadgetContext`). Comment text mentioning `${plugin-data}` or `${plugin-archive}` stays as-is — both tokens are P4 (wire-contract permission-var surface).

- `src-tauri/src/wasm/runtime/state.rs`:
  - Line 8, 16, 43, 94, 95, 131, 141: `PluginState` → `GadgetState`.
  - Line 44: field `plugin_id: String` → `gadget_id: String` (host-internal — not serialized).
  - Lines 50, 58: rustdoc updated for `GadgetState` type references. `${plugin-data}` and `${plugin-archive}` token text in prose stays (P4).

- `src-tauri/src/wasm/runtime/instance.rs`:
  - Lines 8, 34, 58, 69, 82, 86, 142: `PluginState` → `GadgetState`.

- `src-tauri/src/wasm/runtime/engine.rs`, `host/{assets,clipboard,command,frecency,fs,http,logging,mod,opener,paths,platform,settings,sql,website_metadata}.rs`: every reference to `PluginState` → `GadgetState`. Each file enumerated by the grep on `PluginState` (sweep-edit).

- `src-tauri/src/wasm/bindings.rs`:
  - Line 46: `use crate::wasm::runtime::PluginState` → `GadgetState`.
  - Line 48: `impl torchsnap::plugin::types::Host for PluginState {}` becomes `impl torchsnap::plugin::types::Host for GadgetState {}` (the WIT path `torchsnap::plugin::types::Host` stays — it's WIT-generated, P4).
  - Line 247 doc, 315 format, 547, 585, 591 test strings: `torchsnap-plugin://` → `torchsnap-gadget://`. Test strings inside `#[cfg(test)]` are exact byte equals against the runtime string and must change in lockstep with the production string.

- `src-tauri/src/wasm/protocol.rs`:
  - URI scheme lines 8, 14, 16, 17, 47, 56, 265, 451: `torchsnap-plugin://` → `torchsnap-gadget://` (the `torchsnap-plugin` token in line 17's Windows mapping comment also flips).
  - Type-alias line 36 `pub type PluginSourceRegistry` → `GadgetSourceRegistry`. Function `new_registry` (line 39) name unchanged.
  - Trait references at lines 11, 27, 53, 77, 113, 193, 206, 246, 415, 629: `PluginSource` → `GadgetSource`.

- `src-tauri/src/wasm/source.rs`:
  - Lines 7, 18-21, 33, 50, 52, 71, 73, 77, 113, 119, 399: `PluginSource` (trait) → `GadgetSource`, `PluginSourceKind` (enum) → `GadgetSourceKind`. Variants `Builtin/System/User/Dev` and serde lowercase strings unchanged (wire shape preserved).
  - Rustdoc at lines 113 and 399 (`${plugin-archive}`) stays as-is (P4).
  - Line 60-65 rustdoc reference to `<resource_dir>/plugins/` and `<app_data_dir>/plugins/` paths stay (P5).

- `src-tauri/src/wasm/discovery.rs`, `wasm/manifest/{mod,storage,frontend}.rs`, `wasm/runtime/mod.rs`, `wasm/logging/channel.rs`:
  - Type-name uses of `PluginSource`, `PluginSourceKind`, `PluginId`, `PluginMeta`, `PluginIcon`, `WasmPluginManifest` updated. `PluginId` validation prose at `manifest/mod.rs:185-189` updated only where it names the type, not where it says "plugin id" as a domain noun (those strings are wire/format documentation that P2 prose owns).
  - `manifest/frontend.rs:48` rustdoc `torchsnap-plugin://` → `torchsnap-gadget://`.

- `src-tauri/src/lib.rs`:
  - Line 11: `mod plugin_host;` → `mod gadget_host;`.
  - Line 12: `mod plugin_install;` → `mod gadget_install;`.
  - Line 13: `mod plugins;` → `mod gadgets;`.
  - Line 539, 551, 552, 553, 597, 598, 599, 600, 674, 680, 684, 695, 702, 709, 960, 994, 997, 1103, 1143, 1145, 1150: every `plugin_host::*`, `plugins::*`, `plugin_install::*`, `wasm::protocol::PluginSourceRegistry`, `wasm::source::PluginSourceKind`, `WasmPluginBridge` reference rewired to the new module/type paths. Tauri command function identifiers `wasm_plugins`, `plugin_sources`, `plugin_install::install_plugin_archive`, `plugin_install::uninstall_user_plugin` stay as the function *names* (P4) — but `plugin_install::` becomes `gadget_install::` because the *module path* is P3.
    - Concretely: line 599 becomes `gadget_install::install_plugin_archive,`. The function symbol name unchanged remains `install_plugin_archive` for P4.

- `src-tauri/src/commands/mod.rs:20`: `use crate::plugin_host::PluginHost` → `use crate::gadget_host::GadgetHost`. Line 73 Tauri command function name `plugin_message` stays.

- `src-tauri/src/commands/types.rs`:
  - Lines 217-239 `PluginResponse` → `GadgetResponse`.
  - Lines 245-256 `PluginViewRef` → `GadgetViewRef` (struct rename only; field `plugin_id` stays — wire surface).
  - Line 312 `ResultSource::Plugin { id }` enum variant: variant identifier `Plugin` is P4 (it serializes as `"type":"plugin"` per line 326 test). Stays in P3.
  - Lines 293, 297 fields `custom_plugin_view`, `inline_plugin_view` stay (wire surface — P4).

### Module path updates (every `mod`/`use` touching renamed paths)

All `mod plugin*` and `use crate::plugin*::*` / `use crate::plugins::*` declarations are exhaustively listed above. A final sweep with `git grep -nE 'mod plugin|use crate::plugin|use crate::plugins|use super::Plugin'` after the symbol-rename commits ensures none escape.

### rustdoc updates

Inside every renamed `.rs` file, every rustdoc comment that names a renamed *type* is updated to the new name. Domain-language uses of "plugin" (e.g., "the plugin's manifest", "plugin-author-facing", `${plugin-archive}`) are P2 territory and stay in P3. No `cargo doc --no-deps` build is required as part of P3 verification because the reformatted rustdoc is mechanical and `just check` already exercises rustdoc lints.

### Test renames

- `src-tauri/src/gadget_host.rs` (`#[cfg(test)] mod tests`): `MockPlugin` → `MockGadget`, helper `plugin_slots()` → `gadget_slots()`, local `plugin`/`plugins` idents follow.
- `src-tauri/src/wasm/protocol.rs` test helpers (`test_registry`, etc.) follow the trait/type renames.
- `src-tauri/src/wasm/bridge.rs` tests at lines 1082-1518: `WasmPluginBridge::new` → `WasmGadgetBridge::new` everywhere in test bodies.
- `src-tauri/src/wasm/bindings.rs` tests at lines 547, 585, 591: URI scheme updated.
- `src-tauri/tests/fixtures/*-plugin/` directories are NOT renamed — P5.
- Loaded fixture WASM file paths in `wasm/runtime/mod.rs:208,738,741` (`website-metadata-plugin`) and similar in `bridge.rs` stay (P5).

## TypeScript rename plan

### File renames

```
git mv src/contexts/PluginContext.tsx              src/contexts/GadgetContext.tsx
git mv src/contexts/PluginContextProvider.tsx      src/contexts/GadgetContextProvider.tsx
git mv src/contexts/usePluginInfo.ts               src/contexts/useGadgetInfo.ts
git mv src/contexts/usePluginRuntime.ts            src/contexts/useGadgetRuntime.ts
git mv src/contexts/usePluginSetting.ts            src/contexts/useGadgetSetting.ts
git mv src/hooks/usePluginStream.ts                src/hooks/useGadgetStream.ts
git mv src/lib/pluginCss.ts                        src/lib/gadgetCss.ts
git mv src/lib/pluginMessage.ts                    src/lib/gadgetMessage.ts
git mv src/lib/pluginComponent.tsx                 src/lib/gadgetComponent.tsx
git mv src/devtools/console/pluginColors.ts        src/devtools/console/gadgetColors.ts
git mv src/settings/sections/PluginsManagementPanel.tsx \
       src/settings/sections/GadgetsManagementPanel.tsx
git mv src/settings/PluginSettingsWrapper.tsx      src/settings/GadgetSettingsWrapper.tsx
git mv packages/plugin-sdk/src/testing/MockPluginContextProvider.tsx \
       packages/plugin-sdk/src/testing/MockGadgetContextProvider.tsx
```

`src/lib/sdk.ts` is **not** renamed. Its symbol `initPluginSdk` → `initGadgetSdk` per inventory line 53. Filename is generic.

### Symbol renames + barrel export updates

- `src/contexts/index.ts`: every export rewritten —
  ```
  export {
    GadgetContext,
    type GadgetContextValue,
    type GadgetInfo,
    type GadgetRuntime,
    type GadgetSendMessage,
    type LauncherActions,
  } from "./GadgetContext";
  export { GadgetContextProvider, type GadgetContextProviderProps } from "./GadgetContextProvider";
  export { useGadgetInfo } from "./useGadgetInfo";
  export { useGadgetRuntime } from "./useGadgetRuntime";
  export { useLauncher } from "./useLauncher";
  export { useGadgetSetting } from "./useGadgetSetting";
  ```
  `LauncherActions`, `useLauncher` unchanged.

- Inside `GadgetContext.tsx` (formerly `PluginContext.tsx`):
  - `PluginInfo`, `PluginRuntime`, `PluginSendMessage`, `PluginContextValue`, `PluginContext` all renamed to `Gadget*`.
  - The shim sync block at lines 104-124: rename SDK alias imports and tuple checks accordingly. Comment block lines 91-102 updated to refer to the new shim path (`packages/plugin-sdk/src/shims/hooks.ts`'s `GadgetInfo`, etc.).

- `GadgetContextProvider.tsx`: imports follow; component `PluginContextProvider` → `GadgetContextProvider`; props interface follows.

- `useGadgetInfo.ts`, `useGadgetRuntime.ts`, `useGadgetSetting.ts`: function names and import paths follow. Inside `useGadgetSetting.ts:24`, the literal `\`plugins.${id}.${key}\`` **stays** (P4 — settings-key prefix).

- `useGadgetStream.ts` (formerly `usePluginStream.ts`):
  - Imports `PluginSendMessage` → `GadgetSendMessage` (from `../contexts/GadgetContext`).
  - Function `usePluginStream` → `useGadgetStream`. State interface `PluginStreamState` → `GadgetStreamState`.
  - Console error message `usePluginStream(${method})` → `useGadgetStream(${method})`.

- `gadgetCss.ts` (formerly `pluginCss.ts`):
  - Function `injectPluginCss` → `injectGadgetCss`, `removePluginCss` → `removeGadgetCss`. Parameter `pluginId` → `gadgetId`.
  - URI string lines 8 (JSDoc), 25: `torchsnap-plugin://` → `torchsnap-gadget://`.
  - HTML attributes line 36 `[data-plugin="..."]` → `[data-gadget="..."]`; line 39 `style.dataset.pluginCss = pluginId` → `style.dataset.gadgetCss = gadgetId`; line 49 `style[data-plugin-css="..."]` → `style[data-gadget-css="..."]`.

- `gadgetMessage.ts` (formerly `pluginMessage.ts`):
  - Function `sendPluginMessage` → `sendGadgetMessage`. Tauri command name string `"plugin_message"` at line 31 stays (P4).

- `gadgetComponent.tsx` (formerly `pluginComponent.tsx`):
  - Inner factory function `pluginComponent` → `gadgetComponent`. Public exports `launcherComponent`, `settingsComponent` keep their names (they are mode-specific factories, not "plugin"-named); local `launcherLoaders`, `settingsLoaders` keep.

- `gadgetColors.ts` (formerly `pluginColors.ts`):
  - `PLUGIN_COLORS` → `GADGET_COLORS`. `pluginColorIndex(pluginId: string)` → `gadgetColorIndex(gadgetId: string)`. Header rustdoc-style block updated.

- `src/devtools/console/{ConsoleToolbar.tsx,LogList.tsx,LogItemRow.tsx,TreeLogList.tsx}`: import path changes from `./pluginColors` → `./gadgetColors`; `PLUGIN_COLORS`/`pluginColorIndex` references follow. Local `pluginId` arguments at TreeLogList.tsx:90, ConsoleToolbar.tsx:203, LogItemRow.tsx:140 rename to `gadgetId`. `formatters.ts:44` string compare `"plugin"` stays (P4).

- `src/lib/sdk.ts`:
  - Function `initPluginSdk` → `initGadgetSdk`.
  - Imports of host hooks updated to `useGadgetInfo`, `useGadgetRuntime`, `useGadgetSetting`.
  - `window.__torchsnap.hooks` property names `usePluginInfo`, `usePluginRuntime`, `usePluginSetting` → `useGadgetInfo`, `useGadgetRuntime`, `useGadgetSetting`. (Type interface `TorchsnapGlobal.hooks` follows; the global itself remains `__torchsnap` — that's the product name.)
  - The shim that re-reads from the global at `packages/plugin-sdk/src/shims/hooks.ts` must rename in lockstep — the runtime contract is the property names.

- `src/lib/command.ts`:
  - Line 53 `plugin_message`, line 102 `wasm_plugins`, line 103 `plugin_sources`, line 104 `install_plugin_archive`, line 108 `uninstall_user_plugin` Tauri command-name keys stay (P4 — wire surface).
  - Line 142 `export type PluginSourceKind` → `GadgetSourceKind`.
  - Line 151 `export interface WasmPluginManifest` → `WasmGadgetManifest`. The inner field `plugin: { ... }` (line 152) stays — that field name mirrors `manifest.toml`'s `[plugin]` section header, P4 territory.

- `src/plugins/types.ts`: type-only file inside the to-be-renamed-in-P5 `src/plugins/` directory. Symbol renames performed in P3:
  - `PluginViewProps` → `GadgetViewProps`, `PluginSettingsProps` → `GadgetSettingsProps`, `PluginViewRef` → `GadgetViewRef` (TS counterpart). `InlineViewProps` unchanged. JSDoc updated to refer to the new names; the `pluginId: string` field in `PluginViewRef` stays (it's the wire field — P4 rename of TS type's field). Add a comment noting P4 will rename the field.

- `src/plugins/registry.ts`:
  - `PluginRegistryEntry` → `GadgetRegistryEntry`. Functions `registerPlugin` → `registerGadget`, `unregisterPlugin` → `unregisterGadget`, `getPluginView` → `getGadgetView`, `getPluginInlineView` → `getGadgetInlineView`, `getPluginSettingsComponent` → `getGadgetSettingsComponent`, `getPluginsWithSettings` → `getGadgetsWithSettings`. Local `pluginId` parameter follows. Map variable `registry` keeps. Existing `registerPlugin("app-launcher", ...)` calls update to `registerGadget("app-launcher", ...)`.

- `src/plugins/wasmPluginLoader.ts`:
  - `registerWasmPlugin` → `registerWasmGadget`, `registerAllWasmPlugins` → `registerAllWasmGadgets`. Local `pluginId` → `gadgetId`.
  - `WasmPluginManifest` → `WasmGadgetManifest` (TS type alias): the wire JSON keys are independent of the TS type name, so renaming the TS alias is host-internal and belongs in P3. The Rust counterpart `WasmPluginManifest` is in P3 scope for the same reason (inventory naming map line 39).
  - URI string line 40 `torchsnap-plugin://` → `torchsnap-gadget://`. JSDoc line 10 follows.
  - `PluginViewProps`, `InlineViewProps` import path rewires to renamed types.

- `src/plugins/clipboard/{ClipboardView,ClipboardSettings}.tsx`: imports of `PluginViewProps`, `PluginSettingsProps`, `usePluginInfo`/`usePluginSetting` updated. The clipboard plugin's runtime behavior unchanged.

- `src/launcher/Launcher.tsx`:
  - Lines 10-22: imports rewired: `sendGadgetMessage`, `GadgetContextProvider`, `LauncherActions, GadgetInfo, GadgetRuntime`, `getGadgetView, getGadgetInlineView`, `GadgetViewProps, InlineViewProps, GadgetViewRef`.
  - Local `pluginId: string` parameter and `PluginViewRef` use rename to `gadgetId` and `GadgetViewRef`. The named React state locals `customPluginView`, `inlinePluginView`, `customPluginViewRef`, `executePluginView`, `searchPluginView` rename to `customGadgetView`, etc.
  - Line 94, 119 `<div data-plugin={pluginId}>` → `<div data-gadget={gadgetId}>`.
  - Line 226-233 listening for `"activate-plugin-custom-ui"` — wire event name stays in P3 (P4 owns the rename).
  - Lines 363, 450 use `customPluginViewRef` — local rename.

- `src/launcher/main.tsx`, `src/settings/main.tsx`:
  - `preloadLauncherComponents`, `preloadSettingsComponents` keep their public names (factories that work for any host-loaded component, not gadget-specific).
  - `initPluginSdk` → `initGadgetSdk`. `registerAllWasmPlugins` → `registerAllWasmGadgets`.
  - Local `wasmPlugins` variable renames to `wasmGadgets`.

- `src/settings/SettingsPanel.tsx`:
  - Imports rewired: `getGadgetSettingsComponent, getGadgetsWithSettings`, `GadgetSettingsProps`, `GadgetContextProvider`, `GadgetInfo, GadgetRuntime`, `sendGadgetMessage`.
  - Local variable `pluginSections` → `gadgetSections`, `plugin` parameter follows. The string keys passed to `useSetting` (`enabled.<id>`, `plugins.<id>.*`) stay (P4).

- `src/settings/sections/GadgetsManagementPanel.tsx` (formerly `PluginsManagementPanel.tsx`, renamed via `git mv` — see File renames above):
  - Component name `PluginsManagementPanel` → `GadgetsManagementPanel`. Imports rewired to `getGadgetsWithSettings`. All consumers (`SettingsPanel.tsx` or wherever the component is imported) updated.

- `src/settings/GadgetSettingsWrapper.tsx` (formerly `PluginSettingsWrapper.tsx`, renamed via `git mv` — see File renames above):
  - Component name `PluginSettingsWrapper` → `GadgetSettingsWrapper`. Imports updated; all consumers rewired.

- `src/launcher/hooks/useSearch.ts`:
  - Lines 27, 35, 37, 45, 46: `PluginViewRef` → `GadgetViewRef`. Local `customPluginView`, `inlinePluginView` follow.

- `src/types.ts`:
  - Lines 102, 135, 137: type `PluginViewRef` → `GadgetViewRef`; field names `customPluginView`/`inlinePluginView` stay if they correspond to wire payload (verify via `src-tauri/src/commands/types.rs:293,297` — yes, those serialize as `customPluginView`/`inlinePluginView` per `rename_all = "camelCase"` on Rust side; wire surface — P4). The TypeScript *type* rename (struct identifier) is P3; the *field* names stay for P4.

- `packages/plugin-sdk/src/shims/hooks.ts`:
  - Lines 47-59 `interface TorchsnapGlobal { hooks: { usePluginInfo, usePluginRuntime, useLauncher, usePluginSetting, useWindowedList } }` — properties `usePluginInfo`/`usePluginRuntime`/`usePluginSetting` → `useGadget*`.
  - Lines 81-131 type aliases `PluginInfo`, `PluginRuntime`, `PluginSendMessage` → `GadgetInfo`, `GadgetRuntime`, `GadgetSendMessage`.
  - Lines 154-170 exported shim hooks `usePluginInfo`, etc. → `useGadgetInfo`, etc.
  - JSDoc on `PluginSendMessage` (lines 86-108) mentions `WasmPluginBridge` → `WasmGadgetBridge`.

- `packages/plugin-sdk/src/testing/MockGadgetContextProvider.tsx` (formerly `MockPluginContextProvider.tsx`):
  - Component name and props interface renamed. Code-block JSDoc examples updated (lines 18-27).

- `packages/plugin-sdk/src/testing/index.ts`: re-exports follow.

- `packages/plugin-sdk/src/types/{plugin.ts,index.ts,settings.ts}`: any `Plugin*` type aliases referenced from the host shim sync block follow; the npm package's published `package.json` exports map and name `@torchsnap/plugin-sdk` stay (P4/P5).

### JSDoc / comment updates inside renamed files

Mechanical: every JSDoc reference to a renamed symbol updated. Domain-language uses of "plugin" (the noun for what end users author) stay until P2 prose owns them.

### Test renames

- No test file is renamed in P3 except where its filename is a verbatim mirror of a renamed symbol. None of the existing TS test files (`*.test.tsx`) under `packages/plugin-sdk/` have such mirror filenames; they reference `MockPluginContextProvider` by import path. Each test file gets its imports rewired.
- New tests are not introduced — see Test impact below.

## URI scheme rename plan

Two sides change atomically in one commit:

- **Rust registration (server)**: `src-tauri/src/wasm/protocol.rs` registers the `torchsnap-plugin://` scheme via Tauri's `register_uri_scheme_protocol`. Lines 8, 14, 16, 17, 47, 56, 265, 451 carry the literal `torchsnap-plugin` (rustdoc, registration string, URL constructions, test fixtures) — every occurrence rewrites to `torchsnap-gadget`.
- **Rust callers**: `src-tauri/src/wasm/bindings.rs` lines 247 (doc), 315 (format!), 547, 585, 591 (test strings). `src-tauri/src/wasm/manifest/frontend.rs:48` (rustdoc).
- **TS callers**: `src/lib/pluginCss.ts` (renamed `gadgetCss.ts`) line 25 (URL construction), `src/plugins/wasmPluginLoader.ts` line 40, `src/components/Icon.tsx` lines 16, 74 (JSDoc only).
- **Test fixtures**: no fixture file or `manifest.toml` references `torchsnap-plugin://`. Confirmed by grep.

Verification command after the URI commit: `git grep -F 'torchsnap-plugin'` returns zero matches in `src-tauri/`, `src/`, except in P4-pending wire-event names (none) — and zero matches outside docs (P2).

## Commit grouping

Eight ordered, atomic, buildable commits. Each follows the project commit workflow exactly: **Write tool to `.tmp-commit-msg`, then `git commit -F .tmp-commit-msg`, then `rm .tmp-commit-msg` — three independent tool calls, never chained**.

1. **Rename PluginHost / PluginSlot / Plugin trait and host built-in module** — `src-tauri/src/plugin_host.rs` (renamed) + `src-tauri/src/plugins/` directory rename + every `mod`/`use` consumer in `src-tauri/src/lib.rs`, `commands/mod.rs`, `wasm/bridge.rs`. All host built-in struct renames (`AppLauncherPlugin`, `BuiltInCommandsPlugin`, `ClipboardPlugin`, `SystemCommandsPlugin`, `SystemPreferencesPlugin`) land here so `cargo check` stays green at end-of-commit.
2. **Rename PluginInstall / InstalledPluginInfo** — `src-tauri/src/plugin_install.rs` (renamed) + `src-tauri/src/lib.rs` invoke handler entries.
3. **Rename WASM runtime types: PluginState, WasmPluginBridge, WasmPluginManifest, PluginId, PluginMeta, PluginSourceKind, PluginSourceRegistry, PluginSource trait, PluginResponse, PluginViewRef, PluginShortcut, PluginContext, PluginSettings, ActivatePluginPayload** — `src-tauri/src/wasm/`, `src-tauri/src/commands/types.rs`, `src-tauri/src/settings/mod.rs`, `src-tauri/src/plugin_host.rs` (now `gadget_host.rs`).
4. **Rename PluginFrecency** — `src-tauri/src/frecency/plugin_frecency.rs` (renamed) + `frecency/mod.rs` re-export + every consumer.
5. **Rename TS contexts + hooks** — every file under `src/contexts/`, `src/hooks/usePluginStream.ts`, plus `src/contexts/index.ts` barrel. Includes the SDK shim type-alias rename in `packages/plugin-sdk/src/shims/hooks.ts` (and the testing/types files that import from it) so `bun typecheck` stays green at end-of-commit. The `MockPluginContextProvider.tsx` file rename rides this commit.
6. **Rename TS lib internals: sdk.ts symbols, pluginCss/Message/Component file renames** — file renames + symbol renames + every consumer in `src/launcher/`, `src/settings/`, `src/plugins/`. Includes `src/lib/command.ts`'s `PluginSourceKind` and `WasmPluginManifest` TS aliases (independent of the Rust counterparts in commit 3 — TS and Rust each declare their own type) so `bun typecheck` stays green at end-of-commit.
7. **Rename devtools console pluginColors → gadgetColors** — file rename + symbol rename + four consumers (`ConsoleToolbar.tsx`, `LogList.tsx`, `LogItemRow.tsx`, `TreeLogList.tsx`).
8. **Rename URI scheme `torchsnap-plugin://` → `torchsnap-gadget://`** — Rust registration (`wasm/protocol.rs`) and every TS caller (`gadgetCss.ts`, `wasmPluginLoader.ts`, `Icon.tsx` JSDoc) change in one commit so no caller constructs a stale URL. HTML attributes `data-plugin`/`data-plugin-css` (Launcher.tsx + already-renamed `gadgetCss.ts`) ride this commit since they share the host↔host-rendered-content contract.

Each commit subject is title-only (no semantic prefix), e.g.:
- `Rename PluginHost to GadgetHost and migrate host built-in module to gadgets/`
- `Rename plugin_install module to gadget_install with InstalledGadgetInfo`
- `Rename WASM runtime types from Plugin* to Gadget*`
- `Rename PluginFrecency to GadgetFrecency`
- `Rename TS context and hook surfaces from Plugin to Gadget`
- `Rename pluginCss/pluginMessage/pluginComponent and SDK init to Gadget*`
- `Rename devtools console pluginColors palette to gadgetColors`
- `Rename torchsnap-plugin URI scheme to torchsnap-gadget and flip data-plugin HTML attributes`

## Test impact

### Existing Rust tests

- `src-tauri/src/gadget_host.rs` `#[cfg(test)] mod tests` (lines 1010-1450 region in the original): `MockPlugin`, `plugin_slots`, helper local `plugin`/`plugins` idents — renamed in lockstep with the type rename in commit (1).
- `src-tauri/src/wasm/bridge.rs` test functions calling `WasmPluginBridge::new` — lines 1082, 1089, 1132, 1176, 1317, 1360, 1395, 1518 — renamed in commit (3).
- `src-tauri/src/wasm/bindings.rs` URI scheme tests at lines 547, 585, 591 — renamed in commit (8).
- `src-tauri/src/wasm/protocol.rs` `test_registry`, scheme-handler tests — renamed in commits (3) and (8).
- `src-tauri/src/commands/types.rs:316-346` `result_source_tests`: serializer test asserts `{"type":"plugin","id":...}` — **stays in P3** (wire surface, P4).
- `src-tauri/src/wasm/runtime/mod.rs:208,738,741` `WEBSITE_METADATA_PLUGIN_WASM` constant + `compile("website-metadata-plugin", ...)` and `instantiate("website-metadata-plugin")` — stay (fixture path is P5).

### Existing TS tests

- `packages/plugin-sdk/src/testing/MockGadgetContextProvider.tsx` (renamed in commit 5) is consumed by any plugin test using `bun test`. Tests under `plugins/<id>/frontend/__tests__/` are P5 territory. Tests inside `packages/plugin-sdk/src/` itself: imports rewired in commit (5).
- No `*.test.ts(x)` file under `src/` references the renamed symbols; the host has no Bun unit tests against the context (host correctness is exercised through Rust integration tests).

### New tests

None justified per subsystem:

- **Rust file/type renames**: mechanical; existing `mod tests` continues to exercise the renamed types. The compiler itself is the strongest test that no consumer was missed.
- **TS file/type renames**: TypeScript's strict mode and the host's `noUncheckedIndexedAccess`/`strict` ESLint rules catch every dangling import or symbol reference at `bun typecheck`.
- **URI scheme rename**: existing protocol tests at `wasm/protocol.rs` and `wasm/bindings.rs:547,585,591` already exercise the protocol round-trip; updating their string literals validates the new scheme end-to-end.

A new test would be justified only if the rename introduced *behavior* — it does not. (Per inventory.md "Tests and docs coverage" note: new tests are required only "where the rename creates a behavioural risk".)

## Documentation impact within Rust/TS source

- Rustdoc on every renamed Rust file's headers and trait/struct/enum/fn doc-comments updated to refer to the renamed type. Domain-noun "plugin" stays (P2 owns prose).
- JSDoc on every renamed TS file (`GadgetContext.tsx`, `GadgetContextProvider.tsx`, `useGadget*.ts`, `useGadgetStream.ts`, `gadgetCss.ts`, `gadgetMessage.ts`, `gadgetComponent.tsx`, `gadgetColors.ts`) updated. Examples in JSDoc code blocks updated to the new symbol names.
- The shim sync comment block at `src/contexts/GadgetContext.tsx:91-102` updated: it points readers at `packages/plugin-sdk/src/shims/hooks.ts` — the path stays (P5 owns directory rename) but the type-alias names referenced in the prose are updated.
- `MockGadgetContextProvider.tsx` JSDoc example block (lines 18-27) updated.

Standalone `.md` documents in `docs/`, ADRs, README — **all P2**.

## Verification steps

After each commit (and definitely before pushing):

1. `just check` — runs `cargo check` for both `src-tauri` and the `plugins/` virtual workspace plus `bun typecheck`. Catches every dangling Rust `use` and every TS import/symbol-resolution failure.
2. `just test` — runs `cargo test --workspace -p torchsnap_lib` (host integration tests, including `mod tests` blocks in the renamed files) and `bun test` (TS shim/test surfaces).
3. `just lint` — clippy + ESLint on the renamed surface.
4. `just check-wit` and `just fmt-wit` — confirm WIT untouched (defensive: P3 must NOT touch WIT).
5. `just start` — boot the launcher in debug mode, exercise: open launcher, type a query, trigger one host built-in (e.g. clipboard), open settings, install/uninstall a user plugin if available. The WASM URI scheme `torchsnap-gadget://` is exercised the moment a WASM plugin's frontend bundle is requested.
6. After commit (8): `git grep -F 'torchsnap-plugin'` returns zero matches outside `docs/`, `todos/`, ADRs, and the WIT/SDK-name surface (which P4/P5 own).
7. After commit (3): `git grep -nE 'PluginHost|PluginSlot|PluginSource|PluginState|PluginId|PluginFrecency|PluginContext|PluginShortcut|PluginResponse|PluginViewRef|PluginMeta|PluginSettings|WasmPluginBridge|WasmPluginManifest|InstalledPluginInfo|AppLauncherPlugin|BuiltInCommandsPlugin|ClipboardPlugin|SystemCommandsPlugin|SystemPreferencesPlugin|MockPlugin|ActivatePluginPayload' src-tauri/src/` returns zero matches.
8. After commit (5/6): `git grep -nE 'PluginContext|PluginContextProvider|PluginInfo|PluginRuntime|PluginSendMessage|usePluginInfo|usePluginRuntime|usePluginSetting|usePluginStream|injectPluginCss|removePluginCss|sendPluginMessage|initPluginSdk|MockPluginContextProvider' src/ packages/` returns zero matches.

## Risks & rollback

### Top risk: missed `use crate::plugins::*` import

The host built-ins module rename `src/plugins/` → `src/gadgets/` and the trait rename `Plugin` → `Gadget` together touch the largest cross-module surface. Every import of `crate::plugins::Plugin` and `super::{Plugin, PluginContext}` must rewire. Verification: `git grep -nE 'crate::plugins|super::Plugin|::Plugin\b'` after commit (1) must return zero matches inside `src-tauri/`. If `cargo check` succeeds the import set is consistent — the compiler is the authoritative check.

### Second risk: compile-time shim sync drift

The compile-time `extends` checks at `src/contexts/GadgetContext.tsx:104-124` (post-rename) bind the host's `GadgetInfo`/`GadgetRuntime`/`GadgetSendMessage`/`LauncherActions` to the SDK shim's `Sdk*` re-imports. If commit (5) renames the host side without renaming the shim side, `bun typecheck` fails immediately — that's by design. Recovery: continue editing within the same commit; do not amend across commits.

### Third risk: stranded URI scheme registration

If commit (8) renames the registration but a stale TS caller still constructs `torchsnap-plugin://`, every plugin frontend asset 404s at runtime, with no compile-time warning. Mitigation: commit (8) edits the Rust registration AND every TS caller AND the test fixtures in lockstep; verification step 6 (`git grep -F 'torchsnap-plugin'`) is mandatory before declaring the commit done.

### Rollback approach

Every commit is independently buildable. If a problem is caught only after a later commit lands, a `git revert` of the offending commit restores the prior state. No multi-commit revert required because each commit groups one self-contained subsystem. The repo is pre-alpha (per inventory.md decision context), so a `git reset --hard` to before the phase is acceptable if necessary — the user is the sole consumer.

---

### Critical Files for Implementation

- /Users/jakob/Development/github/jakobwesthoff/torchsnap/src-tauri/src/plugin_host.rs
- /Users/jakob/Development/github/jakobwesthoff/torchsnap/src-tauri/src/wasm/bridge.rs
- /Users/jakob/Development/github/jakobwesthoff/torchsnap/src-tauri/src/wasm/protocol.rs
- /Users/jakob/Development/github/jakobwesthoff/torchsnap/src/contexts/PluginContext.tsx
- /Users/jakob/Development/github/jakobwesthoff/torchsnap/packages/plugin-sdk/src/shims/hooks.ts
