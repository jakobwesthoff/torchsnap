# Phase 4 — Public/Wire API Surface

## Summary

Phase 4 renames every name that crosses a boundary visible to gadget authors or to the WASM guest runtime: the WIT package/world/file, the Rust SDK crate and its `define_plugin!` macro, the npm `@torchsnap/plugin-sdk` package and its deep-import paths, the five Tauri command names and their TS registry, the Tauri event names `open-plugin-settings` and `activate-plugin-custom-ui` plus their payload keys, the settings-store key prefix `plugins.<id>.`, the `ResultSource` and `LogSource` wire discriminator strings, the manifest permission-var tokens `${plugin-data}` and `${plugin-archive}`, the `bundled.toml` top-level `plugins = […]` key, and the `[plugin]` section name inside every `manifest.toml`. P4 is otherwise mechanical — every guest type rename and every internal Rust/TS identifier rename has already happened in P3. P4 is what makes the rename observable across the IPC, WIT, settings, and on-disk surfaces.

## Predecessors

- **P3 must have landed.** P4 changes Rust `#[tauri::command]` function bodies and `Manifest`/`PluginMeta` field bindings that reference Rust types P3 already renamed (`Gadget*` family, `GadgetHost`, `GadgetSourceKind`, `GadgetMeta`, etc.). Without P3, the renamed Tauri command bodies still reference `PluginHost`/`PluginSourceKind`/etc. Likewise, `LogSource::Plugin(...)` callers across `src-tauri/src/wasm/**/*.rs` were renamed to the new internal variant name in P3 (the *Rust enum variant* `Plugin` → `Gadget`); P4 only changes the **serde rename**/discriminator string, not the enum variant identifier.
- **P2 (docs) does not block P4.** P4 still updates the rustdoc/jsdoc inside renamed files, but the prose-only doc tree is independent.
- **P1 (UI strings) does not block P4.**

## Successors

- **P5 directly depends on P4.** P5 moves directories (`plugins/` → `gadgets/`, `plugins/plugin-sdk/` → `gadgets/gadget-sdk/`, `packages/plugin-sdk/` → `packages/gadget-sdk/`, fixture dirs, `target/bundled-plugins/` → `target/bundled-gadgets/`), renames Just recipes and `tools/list-bundled-plugins`, and updates `tauri.conf.json` resources and app-data path strings (`<app_data>/plugins/`, `<app_data>/plugin-home/`). Doing P5 before P4 would force P4 to also chase moved paths — keep them sequential.
- P4 does **not** rename the WIT directory (`wit/`), the WIT-SDK directory (`plugin-sdk/`), the package dir (`packages/plugin-sdk/`), or the staging dir. Only the `.wit` file inside the directory renames.

## Scope

### In scope (with item-by-item justification)

1. **WIT file + package + world.**
   - File rename `plugins/plugin-sdk/wit/torchsnap-plugin.wit` → `plugins/plugin-sdk/wit/torchsnap-gadget.wit`. The filename is wire-surface in the sense that `wit-tools` and downstream tooling resolve the package by file content, but the file is *also* what `wit_bindgen::generate!`'s `path: "wit"` walks. Renaming the file inside the directory is mechanically safe and keeps the SDK's public WIT surface aligned with its package name. (Decision: P4 renames the file. Directory rename `plugins/plugin-sdk/` → `gadgets/gadget-sdk/` stays in P5.)
   - `package torchsnap:plugin@0.1.0;` → `package torchsnap:gadget@0.1.0;`. Public package coordinate.
   - `world plugin { ... }` → `world gadget { ... }`. Public world name.
   - `use torchsnap:plugin/...` resolution (none currently appear inside the WIT — only `use types.{...}` cross-interface uses, which are unaffected).
   - `wit_bindgen::generate!` invocation in `plugins/plugin-sdk/src/lib.rs:43–48` — three coupled args: `path: "wit"` (unchanged; same dir), `world: "plugin"` → `"gadget"`, `default_bindings_module: "::torchsnap_plugin_sdk"` → `"::torchsnap_gadget_sdk"`. Plus the implicit `pub use exports::torchsnap::plugin::...` re-exports that depend on the WIT package name (`exports::torchsnap::gadget::...`) — see SDK plan below.
   - `just check-wit` / `just fmt-wit`: confirmed no recipe-internal wiring depends on the literal filename — both run `wasm-tools` against `plugins/plugin-sdk/wit/` (a directory, picks up the renamed file automatically). Verify in P4 verification step.

2. **Rust SDK crate (Cargo + module path + macro name).**
   - `plugins/plugin-sdk/Cargo.toml` `name = "torchsnap-plugin-sdk"` → `"torchsnap-gadget-sdk"`.
   - Module path `torchsnap_plugin_sdk` → `torchsnap_gadget_sdk` in every consumer (`use` lines, fully-qualified paths). 7 plugin crates touched (`bangs`, `calculator`, `emoji-picker`, `hello-world`, `open-url`, `template`, `zerotier`); the SDK crate's own rustdoc updates with the same sweep.
   - Macro rename `define_plugin!` → `define_gadget!` at the SDK definition (`plugins/plugin-sdk/src/lib.rs:150`) and at every call site (7 plugins; see "SDK rename plan (Rust)" below).
   - Workspace dep declarations in 7 `plugins/<id>/Cargo.toml`. `plugins/Cargo.toml` has no `[workspace.dependencies]` section, so nothing to update there.
   - Internal SDK module paths that thread through the WIT-generated module tree (`torchsnap::plugin::*`, `exports::torchsnap::plugin::*`) follow the package name change automatically: post-rename they become `torchsnap::gadget::*` and `exports::torchsnap::gadget::*`. The 5 `pub use` lines at `lib.rs:66-101` and the one at `sql.rs:24` need their middle path segment updated.

3. **npm package (name + deep-import paths + Vite plugin name + runtime errors).**
   - `packages/plugin-sdk/package.json` `name: "@torchsnap/plugin-sdk"` → `"@torchsnap/gadget-sdk"`.
   - All `@torchsnap/plugin-sdk` deep-import string references inside the package itself: comments in `src/types/index.ts`, `src/testing/index.ts`, `src/vite/index.ts`, `src/types/plugin.ts`, `src/testing/MockPluginContextProvider.tsx`, `src/testing/setup.ts`, plus the four runtime error messages in `src/shims/{hooks,components,keybindings,utils}.ts`. Each error mentions the package name and `initPluginSdk()` (the function-name part is P3).
   - `src/vite/index.ts:32` `name: "torchsnap-plugin-sdk"` → `"torchsnap-gadget-sdk"`.
   - All consumers under `plugins/<id>/frontend/` (`package.json` deps, `vite.config.ts` imports, `*.tsx` imports) — five plugin frontends.
   - All consumers under `src/` (host) — `src/plugins/types.ts:7` (jsdoc reference) and `src/lib/sdk.ts:17` (jsdoc reference).
   - **Note:** `Plugin` from `import type { Plugin } from "vite"` is Vite's own type and is NOT renamed (per inventory line 96–98).

4. **Tauri commands (5 commands, Rust + TS sides + registry).**
   - `wasm_plugins`, `plugin_sources`, `plugin_message`, `install_plugin_archive`, `uninstall_user_plugin` → `wasm_gadgets`, `gadget_sources`, `gadget_message`, `install_gadget_archive`, `uninstall_user_gadget`. Both the `#[tauri::command] fn …` Rust function name **and** every TS `command("…", ...)` call site, plus `src/lib/command.ts` `CommandMap` keys.

5. **Tauri event names and payload keys.**
   - Event names `open-plugin-settings` → `open-gadget-settings`, `activate-plugin-custom-ui` → `activate-gadget-custom-ui`. Both are wire surface — string-matched between `app.emit(...)` (Rust) and `listen(...)` (TS).
   - Payload key `pluginId` → `gadgetId` in both payloads: the `open-gadget-settings` payload (`src-tauri/src/plugin_host.rs:743–745`, inline JSON) and the `ActivateGadgetPayload` struct (`plugin_host.rs:73–80`, derives camelCase).

6. **Settings-store key prefix.**
   - `plugins.<id>.<key>` → `gadgets.<id>.<key>`. Four emit/read sites in `plugin_host.rs` (lines 198, 241, 411, 845) plus deletion of the migration block at lines 206–215 (the entire `if self.store.get(&enabled_key).is_none() { … }` guard, including the leading `if` line and the closing brace, since the missing-key default is already handled by the `.unwrap_or(true)` read at lines 218–222), two sites in `plugin_install.rs` (lines 255, 306), plus the test helper at `plugin_install.rs:304` and the four test bodies at lines 322–365. Plus the WIT rustdoc at `torchsnap-plugin.wit:148` ("`plugins.<plugin-id>.`") which becomes `gadgets.<gadget-id>.`. TS code does not type the literal `"plugins."` prefix (`useGadgetSetting`, P3-renamed, reads through the host hook impl).
   - The `key.strip_prefix("plugins.")` at `plugin_host.rs:845` is the parsing pivot; it changes to `strip_prefix("gadgets.")`. The migration fallback block at lines 206–215 (reads `plugins.<id>.enabled` and copies to `enabled.<id>`) is **deleted entirely** per the no-migration decision — it is not renamed.

7. **Manifest permission-var tokens `${plugin-data}` and `${plugin-archive}`.**
   - `src-tauri/src/wasm/permission_vars.rs:46` `"plugin-data"` → `"gadget-data"` in `RECOGNIZED_PERMISSION_VARIABLES`.
   - `src-tauri/src/wasm/permission_vars.rs:47` `"plugin-archive"` → `"gadget-archive"` in `RECOGNIZED_PERMISSION_VARIABLES`.
   - `src-tauri/src/wasm/permission_vars.rs:71,72` match arms `"plugin-data"` → `"gadget-data"` and `"plugin-archive"` → `"gadget-archive"` in `PathContext::lookup`.
   - `src-tauri/src/wasm/permission_vars.rs` struct fields `plugin_data: PathBuf` → `gadget_data: PathBuf` and `plugin_archive: PathBuf` → `gadget_archive: PathBuf` in `PathContext`. These fields are the host-side sink of the `${plugin-data}` / `${plugin-archive}` substitution — renaming the token and the field is one atomic change, not a P3 symbol rename.
   - `src-tauri/src/wasm/manifest/permissions/command.rs:537` `"plugin-data"` → `"gadget-data"` and `:538` `"plugin-archive"` → `"gadget-archive"` (test fixture array).
   - `src-tauri/src/wasm/runtime/host/fs.rs`: field references `plugin_data:` → `gadget_data:` and the literal `root.join("plugin-data")` → `root.join("gadget-data")` (and corresponding `plugin-archive` → `gadget-archive`) in the test helper `ctx_for`.
   - Every `${plugin-data}` / `${plugin-archive}` reference in rustdoc/comments inside `permission_vars.rs`, `bridge.rs`, `source.rs`, `argv_matcher.rs`, and any other touched files.
   - Test fixtures in `src-tauri/tests/fixtures/*-plugin/manifest.toml` that contain `${plugin-data}` or `${plugin-archive}` permission rules — update manifest content; directory names stay until P5.
   - Any `plugins/<id>/manifest.toml` that uses these tokens — update manifest content.

8. **Source discriminator wire strings (both enums).**
   - `ResultSource::Plugin { id }` (`src-tauri/src/commands/types.rs:309–314`) emits `{"type":"plugin","id":...}` → `{"type":"gadget","id":...}`. Test assertion at `commands/types.rs:326` plus the round-trip test at line 343. TS comparator at `src/types.ts:120`.
   - `LogSource::Plugin(String)` (`src-tauri/src/wasm/logging/mod.rs:88–95`) with `#[serde(tag = "type", content = "value", rename_all = "camelCase")]` emits `{"type":"plugin","value":"<id>"}` → `{"type":"gadget","value":"<id>"}`. TS comparators in `src/devtools/types.ts:17`, `src/devtools/console/formatters.ts:44`, `src/devtools/console/useLogFilters.ts:87,137`, `src/devtools/console/TreeLogList.tsx:59`, `src/devtools/console/LogItemRow.tsx:82`. Plus serde-roundtrip test in `wasm/logging/mod.rs` at line 257 (asserts `"plugin"` shape — must flip to `"gadget"`).

9. **`bundled.toml` key.**
   - `plugins/bundled.toml` line 13 `plugins = […]` → `gadgets = […]`.
   - Comments in same file (lines 1, 3, 5, 7, 13) — also touched, though those are P2-flavored docstrings; rolling them in here keeps the file coherent.
   - Parser updates: `tools/list-bundled-plugins` line 50 `(config as Record<string, unknown>).plugins` → `.gadgets`; line 24 default path is `"plugins/bundled.toml"` (the **directory** part is P5; the filename part stays). Plus diagnostic strings on lines 8, 12, 28–37, 50, 54, 64, 76, 87 that mention "plugin"/"plugins" — these are author-facing, refresh them.
   - `just/plugins.just` uses the same tool; recipe-name changes are P5, but the recipe **body** (line 183 `tools/list-bundled-plugins`) reads the new key automatically once `tools/list-bundled-plugins` is updated.

10. **`manifest.toml` `[plugin]` section name.**
   - Inside the Rust parser: `src-tauri/src/wasm/manifest/mod.rs:58–59`, the field `pub plugin: PluginMeta` becomes `pub gadget: GadgetMeta` (P3 already renamed the type to `GadgetMeta`; P4 renames the field — the field name **is** the TOML key for the inner table). 20+ inline test fixtures in the same file (lines 363, 531, 549, 564, 614, 661, 693, 706, 719, 732, 745, 758, 798, …) carry literal `[plugin]` headings that must change in lockstep.
   - 7 `plugins/<id>/manifest.toml` files (`bangs`, `calculator`, `emoji-picker`, `hello-world`, `open-url`, `template`, `zerotier`).
   - 6 fixture manifests under `src-tauri/tests/fixtures/{assets,command,failing-enable,minimal,opener-http,website-metadata}-plugin/manifest.toml`. (Directory rename `*-plugin/` → `*-gadget/` is **P5**; only the file content changes here.)
   - WIT prose at `torchsnap-plugin.wit` line ~196 mentions `[permissions.opener]` and similar — those are sub-tables and unchanged.

### Out of scope (other phases)

- UI strings in `src/**/*.tsx` (P1).
- Markdown docs (P2). Rustdoc/jsdoc *inside files renamed in P4* are touched as part of the same atomic rename here — that's mechanical, not a doc rewrite.
- Internal Rust/TS type names (P3): `PluginHost`, `PluginSlot`, `PluginContext`, `usePluginSetting`, etc. P4 assumes these are already `Gadget*`.
- Workspace directory layout (P5): `plugins/`, `plugins/plugin-sdk/`, `packages/plugin-sdk/`, fixture dir names, Just recipe names, `target/bundled-plugins/`, app-data path strings.
- App-data path strings `<app_data>/plugins/` and `<app_data>/plugin-home/` (P5).
- URI scheme `torchsnap-plugin://` (P3, internal between host and host-loaded code).

## Open questions

All resolved.

## WIT rename plan

### File: `plugins/plugin-sdk/wit/torchsnap-plugin.wit` → `torchsnap-gadget.wit`

Use `git mv plugins/plugin-sdk/wit/torchsnap-plugin.wit plugins/plugin-sdk/wit/torchsnap-gadget.wit`. Then content edits:

- Line 5: `package torchsnap:plugin@0.1.0;` → `package torchsnap:gadget@0.1.0;`.
- Line 895: `world plugin {` → `world gadget {`.
- All inline rustdoc-style prose: lines 1–2 ("Torchsnap Plugin Interface"), 7, 36–48, 57–62, 110–122, 145–166, 199 (interface opener), 226 (`open-url`), 327–365 (fs section), 421–462 (assets), and other prose mentioning "plugin/plugins". Mechanical rewrite to "gadget/gadgets" inside the renamed file. Those text changes are doc-flavoured but live inside the WIT file we are renaming, so they ride along with the package/world rename.
- Line 148: doc reference to `plugins.<plugin-id>.` becomes `gadgets.<gadget-id>.`.
- The `[permissions.opener]`, `[storage.sql]`, etc. TOML fragments in WIT prose stay (those are sub-tables of `[gadget]`; sub-table names are unchanged).

### Macro: `wit_bindgen::generate!` in `plugins/plugin-sdk/src/lib.rs:43–48`

```
wit_bindgen::generate!({
    path: "wit",                                          // unchanged
    world: "gadget",                                      // was "plugin"
    pub_export_macro: true,                               // unchanged
    default_bindings_module: "::torchsnap_gadget_sdk",    // was "::torchsnap_plugin_sdk"
});
```

The `world: "gadget"` string must match the WIT `world gadget` exactly. The `default_bindings_module` must match the renamed crate name (P4 step 2). Both string literals are coupled and must change together with the WIT and Cargo.toml.

### Generated path consequences inside `plugins/plugin-sdk/src/lib.rs`

The package rename causes the `wit_bindgen` macro to emit modules at `torchsnap::gadget::*` and `exports::torchsnap::gadget::*` instead of `torchsnap::plugin::*` and `exports::torchsnap::plugin::*`. Update:

- Line 66: `pub use exports::torchsnap::plugin::lifecycle::Guest as LifecycleGuest;` → `exports::torchsnap::gadget::lifecycle::Guest`. Same edit for lines 67, 68, 69, 78–81 (search export), 95–101 (`torchsnap::plugin::{…}`).
- `plugins/plugin-sdk/src/sql.rs:24` (`pub use crate::torchsnap::plugin::sql::{…}`) → `crate::torchsnap::gadget::sql::{…}`.

### Plugin-crate-side `use torchsnap_plugin_sdk::…` paths

Module-path renames in plugin crate sources are step 2 of the SDK rename below. They are independent of the WIT package rename — the SDK re-exports under `torchsnap_gadget_sdk::prelude::*` etc., so plugin crates only need to swap the **outer** crate name (`torchsnap_plugin_sdk` → `torchsnap_gadget_sdk`); the inner `torchsnap::plugin::*` path is hidden behind the SDK's flat re-exports.

## SDK rename plan (Rust)

### `plugins/plugin-sdk/Cargo.toml`

- Line 2: `name = "torchsnap-plugin-sdk"` → `"torchsnap-gadget-sdk"`.
- The library compiles as `torchsnap_gadget_sdk` automatically (no `[lib] name = …` override needed).

### `plugins/plugin-sdk/src/lib.rs` (and `src/{command,sql,website_metadata}.rs`)

- All rustdoc references to `torchsnap_plugin_sdk` (lines 13, 16, 32, 33, 47, 64, 106, 124) → `torchsnap_gadget_sdk`.
- `wit_bindgen::generate!` args (see WIT plan).
- `pub use` paths (see WIT plan).
- `define_plugin!` macro (`lib.rs:150`) → `define_gadget!`. The doc comment block (lines 139–148) renames "plugin"→"gadget".
- `impl_noop_messaging!` and `impl_noop_tasks!` keep their names per inventory line 42 (the macro names contain no `plugin` token). Their internal error strings ("plugin does not handle messages", "plugin declares no scheduled tasks") at lines 172 and 184 are author-facing diagnostic strings — update to "gadget does not handle messages" / "gadget declares no scheduled tasks" as part of this file's mechanical sweep.
- `prelude` re-export at line 126 (`pub use super::{define_plugin, …}`) → `define_gadget`.

### Per-plugin crate updates (7 crates)

For each of `bangs/`, `calculator/`, `emoji-picker/`, `hello-world/`, `open-url/`, `template/`, `zerotier/`:

- `Cargo.toml`: `torchsnap-plugin-sdk = { path = "../plugin-sdk" }` → `torchsnap-gadget-sdk = { path = "../plugin-sdk" }`. Path stays (P4); dir rename is P5.
- `src/**/*.rs`: every `use torchsnap_plugin_sdk::…` → `use torchsnap_gadget_sdk::…`. Every fully-qualified `torchsnap_plugin_sdk::…` (e.g. `zerotier/src/lib.rs:165, 171, 176, 233, 629`) → `torchsnap_gadget_sdk::…`. Every `define_plugin!(…)` invocation → `define_gadget!(…)`:
  - `open-url/src/lib.rs:52`
  - `calculator/src/lib.rs:85`
  - `template/src/lib.rs:49`
  - `emoji-picker/src/lib.rs:39`
  - `hello-world/src/lib.rs:23`
  - `bangs/src/lib.rs:78`
  - `zerotier/src/lib.rs:44`

### `plugins/Cargo.toml` (workspace root)

- Line 5 comment: `# plugin plus the shared `torchsnap-plugin-sdk` crate.` → `… torchsnap-gadget-sdk crate.`.
- `plugins/Cargo.toml` has no `[workspace.dependencies]` section; nothing else in this file changes.

### `plugins/.cargo/config.toml`

- No SDK-name references; only `target = "wasm32-wasip2"`. Untouched.

## SDK rename plan (TS/npm)

### `packages/plugin-sdk/package.json`

- Line 2: `"name": "@torchsnap/plugin-sdk"` → `"@torchsnap/gadget-sdk"`.
- Exports map keys/values stay (`"./hooks"`, `"./components"`, etc.) — those are subpath identifiers, not "plugin"-named.

### Internal SDK source files (`packages/plugin-sdk/src/**`)

For every literal `@torchsnap/plugin-sdk` string:

- `src/types/index.ts:6, 12, 13` (top-of-file comment, two example imports).
- `src/types/plugin.ts:11`.
- `src/testing/index.ts:6, 13`.
- `src/testing/MockPluginContextProvider.tsx:19, 32`.
- `src/testing/setup.ts:11`.
- `src/vite/index.ts:17` (jsdoc example import) and `:32` (`name: "torchsnap-plugin-sdk"` → `"torchsnap-gadget-sdk"`).
- `src/shims/keybindings.ts:9` (jsdoc) and `:86` (runtime error string `"@torchsnap/plugin-sdk/keybindings: window.__torchsnap is not initialized — call initPluginSdk() before loading plugin bundles"` → `"@torchsnap/gadget-sdk/keybindings: window.__torchsnap is not initialized — call initGadgetSdk() before loading gadget bundles"`. The `initPluginSdk` → `initGadgetSdk` substitution and the "plugin bundles" → "gadget bundles" prose are P3-flavoured but they live inside the literal we are touching for the package rename, so we cover them in this commit).
- `src/shims/utils.ts:9` (jsdoc) and `:57` (runtime error, same shape).
- `src/shims/components.ts:9` (jsdoc) and `:116` (runtime error, same shape).
- `src/shims/hooks.ts:9` (jsdoc) and `:148` (runtime error, same shape). Plus 9 references to "plugin" in jsdoc and the `usePluginInfo`/`PluginInfo`/`PluginRuntime`/`PluginSendMessage` symbol names — those symbols are P3's job; in P4 we touch only `@torchsnap/plugin-sdk` literals and `initPluginSdk`/runtime-error prose.

### Consumers under `src/` (host)

- `src/plugins/types.ts:7` (jsdoc) → `@torchsnap/gadget-sdk`.
- `src/lib/sdk.ts:17` (jsdoc) → `@torchsnap/gadget-sdk`.

### Plugin-frontend consumers (5 frontends)

For each of `plugins/{bangs,calculator,emoji-picker,template,zerotier}/frontend/`:

- `package.json`: `"@torchsnap/plugin-sdk": "file:../../../packages/plugin-sdk"` → `"@torchsnap/gadget-sdk": "file:../../../packages/plugin-sdk"`. Path stays (P4); dir rename is P5.
- `vite.config.ts`: `import { torchsnap } from "@torchsnap/plugin-sdk/vite";` → `from "@torchsnap/gadget-sdk/vite";`. Plus any other deep-import.
- `src/**/*.{ts,tsx}` for each frontend (settings panels, view components, EmojiGrid, BangsSettings, CalculatorSettings, etc.): every `from "@torchsnap/plugin-sdk[/...]"` → `"@torchsnap/gadget-sdk[/...]"`.

### Bun workspace resolution

`bun install` re-resolves `file:` deps from `package.json` automatically once names line up. If a `bun.lock` (or similar lockfile) is in the repo, regenerate it as part of the same commit so install resolves cleanly post-rename.

## Tauri commands rename plan

| Old (Rust + TS) | New | Rust file:line | TS files |
|---|---|---|---|
| `wasm_plugins` | `wasm_gadgets` | `src-tauri/src/lib.rs:537–543, 597` | `src/lib/command.ts:102`, `src/launcher/main.tsx:38`, `src/settings/main.tsx:24` |
| `plugin_sources` | `gadget_sources` | `src-tauri/src/lib.rs:549–554, 598` | `src/lib/command.ts:103`, `src/settings/sections/PluginsManagementPanel.tsx:65` |
| `plugin_message` | `gadget_message` | `src-tauri/src/commands/mod.rs:73–93`; registered at `src-tauri/src/lib.rs:582` | `src/lib/command.ts:53`, `src/lib/pluginMessage.ts:31` |
| `install_plugin_archive` | `install_gadget_archive` | `src-tauri/src/plugin_install.rs:94–95`; registered at `src-tauri/src/lib.rs:599` | `src/lib/command.ts:104`, `src/settings/sections/PluginsManagementPanel.tsx:389` |
| `uninstall_user_plugin` | `uninstall_user_gadget` | `src-tauri/src/plugin_install.rs:184–185`; registered at `src-tauri/src/lib.rs:600` | `src/lib/command.ts:108`, `src/settings/sections/PluginsManagementPanel.tsx:159` |

In each case rename **both** the `#[tauri::command] fn …` Rust function name (Tauri infers the command name from the function name) and the TS string in `command(…)` calls. The `CommandMap` interface keys in `src/lib/command.ts:53–110` are the registry of record on the TS side and must update in lockstep.

P3 is responsible for the `params` inner field renames (`pluginId` → `gadgetId` inside the `uninstall_user_plugin` params type at `command.ts:109`). P4 only does the wire-surface command-name rename. If P3 has not yet renamed the inner `pluginId` param, that's a P3 gap to backfill before P4 lands.

## Event payload rename plan

### Event 1: `open-plugin-settings`

- **Rust emit** (`src-tauri/src/plugin_host.rs:743–746`): Event name `"open-plugin-settings"` → `"open-gadget-settings"`. Payload `{ "pluginId": source }` → `{ "gadgetId": source }`.
- **TS listen**: no `src/**` consumer of this event name exists. Only the Rust emit changes.

### Event 2: `activate-plugin-custom-ui`

- **Rust emit** (`src-tauri/src/plugin_host.rs:980–989` + struct at lines 73–80): Event name `"activate-plugin-custom-ui"` → `"activate-gadget-custom-ui"`. The struct `ActivatePluginPayload` was already renamed to `ActivateGadgetPayload` by P3 (it is internal Rust). The struct field `plugin_id: String` → `gadget_id: String` (P3, internal Rust). The serde `#[serde(rename_all = "camelCase")]` then emits `gadgetId` automatically. **Verify post-P3 the field is `gadget_id`.** If P3 left the field as `plugin_id`, P4 renames the field here.
- **TS listen**: `src/launcher/Launcher.tsx:232–244` — generic `listen<{ pluginId: string; … }>(…)` and the body `event.payload.pluginId`. Update event name and payload field name.

## Settings-key prefix rename plan

Every site that emits or reads `plugins.<id>.…` becomes `gadgets.<id>.…`:

| File | Line | Current | New |
|---|---|---|---|
| `src-tauri/src/plugin_host.rs` | 198 | `format!("plugins.{id}.")` | `format!("gadgets.{id}.")` |
| `src-tauri/src/plugin_host.rs` | 206–215 | legacy migration block (the entire `if self.store.get(&enabled_key).is_none() { … }` guard) | **Delete entirely.** The block reads `plugins.<id>.enabled` and copies to `enabled.<id>`; per the no-migration decision this code is removed, not renamed. The default-true behaviour is preserved by the `.unwrap_or(true)` at lines 218–222. |
| `src-tauri/src/plugin_host.rs` | 241 | `format!("plugins.{id}.{}", s.settings_key)` | `format!("gadgets.{id}.{}", s.settings_key)` |
| `src-tauri/src/plugin_host.rs` | 411 | `format!("plugins.{plugin_id}.{}", decl.settings_key)` | `format!("gadgets.{gadget_id}.{}", decl.settings_key)` (after P3-renamed `plugin_id` local) |
| `src-tauri/src/plugin_host.rs` | 845 | `key.strip_prefix("plugins.")` | `key.strip_prefix("gadgets.")` |
| `src-tauri/src/plugin_install.rs` | 255 | `format!("plugins.{plugin_id}.")` | `format!("gadgets.{gadget_id}.")` |
| `src-tauri/src/plugin_install.rs` | 306 (test helper) | `format!("plugins.{plugin_id}.")` | `format!("gadgets.{gadget_id}.")` |
| `src-tauri/src/plugin_install.rs` | 322–365 (tests) | `"plugins.foo.alpha"` etc. | `"gadgets.foo.alpha"` etc. — see Test impact |
| `plugins/plugin-sdk/wit/torchsnap-gadget.wit` | ~148 | doc string `plugins.<plugin-id>.` | `gadgets.<gadget-id>.` |

The `"plugins.<plugin-id>."` rustdoc inside the WIT (rendered to plugin authors via cargo-doc and the SDK README) is also wire-surface in the documentation sense — it tells gadget authors the namespace they are writing into. It is updated as part of the WIT file rewrite.

TS side: `useGadgetSetting` (P3-renamed) reads the prefix via the host-provided hook impl, which calls into the Tauri-store-mediated key. The TS layer never types the literal `"plugins."` prefix, so no TS file changes for the prefix rename.

## Manifest permission-var tokens rename plan

The `${plugin-data}` and `${plugin-archive}` tokens are the author-facing manifest contract surface — gadget authors write them in `[[permissions.command]]` `argv` literals and in `paths::resolve(...)` calls. Every site that defines, validates, resolves, or tests these token strings changes in lockstep.

### `src-tauri/src/wasm/permission_vars.rs`

- `RECOGNIZED_PERMISSION_VARIABLES` array (lines 46–47): `"plugin-data"` → `"gadget-data"`, `"plugin-archive"` → `"gadget-archive"`.
- `PathContext` struct fields (lines 58–59): `plugin_data: PathBuf` → `gadget_data: PathBuf`, `plugin_archive: PathBuf` → `gadget_archive: PathBuf`. These are the host-side resolved paths — renaming the struct fields is the host counterpart of the token-string rename and must happen atomically with it.
- `PathContext::lookup` match arms (lines 71–72): `"plugin-data"` → `"gadget-data"`, `"plugin-archive"` → `"gadget-archive"`.
- All rustdoc and inline comments in this file that mention `"plugin-data"`, `"plugin-archive"`, `${plugin-data}`, or `${plugin-archive}` — update to the `gadget-*` forms.
- Unit tests in the same file: `ctx()` helper (lines 236–237) uses `PathBuf::from("/data/plug")` and `PathBuf::from("/archive/plug")` — the field names change to `gadget_data` / `gadget_archive`; the path string values are test-local and may stay. Any test that asserts the literal token name (e.g. `validate_rejects_first_unknown_variable_in_chain` at line 278 uses `"${plugin-data}/${unknown}/${home}"`) — the `${plugin-data}` part becomes `${gadget-data}` once the token is renamed; adjust so the test still exercises the intended scenario.

### `src-tauri/src/wasm/manifest/permissions/command.rs`

- Test fixture array at lines 537–538: `"plugin-data"` → `"gadget-data"`, `"plugin-archive"` → `"gadget-archive"`.

### `src-tauri/src/wasm/runtime/host/fs.rs`

- Test helper `ctx_for` (lines 367–373): field names `plugin_data:` → `gadget_data:`, `plugin_archive:` → `gadget_archive:`. The literal directory name strings `root.join("plugin-data")` → `root.join("gadget-data")` and `root.join("plugin-archive")` → `root.join("gadget-archive")`. Both the field name and the path string must change together; the path string is also the directory the test actually creates on disk.

### Other Rust files in `src-tauri/src/wasm/`

All comments, rustdoc, or inline strings in `bridge.rs`, `source.rs`, `argv_matcher.rs`, and any other files in the WASM subsystem that reference `${plugin-data}` or `${plugin-archive}` — update to the `${gadget-data}` / `${gadget-archive}` forms.

### `PathContext` construction sites

Every site that constructs a `PathContext` value (providing the `plugin_data` and `plugin_archive` fields) must update the field names. Find all callers by grepping for `PathContext {` and updating the field names — typically in `bridge.rs` or wherever gadget launch/enable happens.

### `plugins/<id>/manifest.toml` files

Any per-gadget manifest that contains `${plugin-data}` or `${plugin-archive}` in a permission rule: update the token string. (The token appears in the manifest as the author wrote it; the host validates and substitutes it.)

### `src-tauri/tests/fixtures/*-plugin/manifest.toml`

Any fixture manifest that contains `${plugin-data}` or `${plugin-archive}` permission rules: update the token content. Directory names stay until P5.

## Source-discriminator wire-string rename plan

### `ResultSource` (`src-tauri/src/commands/types.rs:309–314`)

- Variant `Plugin { id: String }` was renamed by P3 to `Gadget { id: String }` (internal Rust type). With `#[serde(tag = "type", rename_all = "camelCase")]` (already on the enum), the wire string becomes `"gadget"` automatically — *no extra serde annotation needed* if the variant is `Gadget`. Confirm post-P3 the variant is `Gadget`. If not, add `#[serde(rename = "gadget")]` here.
- Test assertions at lines 326 and 343 update from `"plugin"` → `"gadget"`.
- TS comparator at `src/types.ts:120` (`type ResultSource = { type: "plugin"; id: string } | { type: "catalog" }`) → `{ type: "gadget"; id: string } | { type: "catalog" }`. No other TS comparators test `source.type === "plugin"` against the `ResultSource` shape — all current `"plugin"` discriminator checks belong to `LogSource`.

### `LogSource` (`src-tauri/src/wasm/logging/mod.rs:88–95`)

- Variant `Plugin(String)` was renamed by P3 to `Gadget(String)`. With `#[serde(tag = "type", content = "value", rename_all = "camelCase")]`, the wire string becomes `"gadget"` automatically.
- Round-trip test at `mod.rs:257` updates assertion from `"plugin"` → `"gadget"`.
- TS consumers (all 5):
  - `src/devtools/types.ts:17`: `type LogSource = { type: "plugin"; value: string } | { type: "host" };` → `{ type: "gadget"; value: string } | { type: "host" };`
  - `src/devtools/console/formatters.ts:44`: `item.source.type === "plugin"` → `=== "gadget"`.
  - `src/devtools/console/useLogFilters.ts:87, 137`: same.
  - `src/devtools/console/TreeLogList.tsx:59`: same.
  - `src/devtools/console/LogItemRow.tsx:82`: same.

## bundled.toml + just recipe consumer plan

### `plugins/bundled.toml`

- Line 13: `plugins = [` → `gadgets = [`. Entry strings (`"calculator"`, `"emoji-picker"`, `"bangs"`, `"open-url"`, `"zerotier"`) are gadget IDs — unchanged in P4 (those are the `manifest.toml` `id =` values; not "Plugin" tokens).
- Comment block (lines 1–12): rewrite "Plugins" → "Gadgets", "stage-bundled-plugins" recipe ref stays as a literal until P5 renames the recipe.

### `tools/list-bundled-plugins`

The script body — file rename to `tools/list-bundled-gadgets` is **P5** (per inventory line 84). For P4, only the script's *content* changes:

- Line 24: `"plugins/bundled.toml"` (default arg) — P5 because directory rename.
- Line 50: `(config as Record<string, unknown>).plugins` → `.gadgets`.
- Lines 8, 12, 28–37, 50, 54, 64, 76, 87 — diagnostic strings that say "plugin"/"plugins". Refresh to "gadget"/"gadgets".

### `just/plugins.just`

Recipe names (`stage-bundled-plugins`, `build-plugin`, etc.) are P5. The recipe **bodies** read `tools/list-bundled-plugins` (line 183 of `just/plugins.just`); once that script is updated to read `gadgets = …`, the body change is zero. The recipes' echo/error strings ("Error: bundled.toml lists '$id' but plugins/$id/ does not exist") are author-facing log strings — `plugins/$id/` literal stays in P4 (path is a P5 concern); the prose stays "plugin" until P5 (cohesive recipe rename).

## manifest.toml section-name rename plan

### Parser (`src-tauri/src/wasm/manifest/mod.rs`)

- Line 59: `pub plugin: PluginMeta,` → `pub gadget: GadgetMeta,`. P3 already renamed the type to `GadgetMeta`; P4 renames the *field*, which is the TOML key the section deserializes from.
- Test fixtures embedded in the file (lines 363, 531, 549, 564, 614, 661, 693, 706, 719, 732, 745, 758, 798, and any others — there are 20+): every literal `[plugin]\n` heading inside `r#"..."#` strings → `[gadget]`.
- The unit tests `parse_minimal_manifest`, `parse_full_manifest`, `parse_heroicon`, `parse_asset_icon_*`, `parse_settings_with_mixed_types`, etc. — assertions reading `manifest.plugin.id` etc. become `manifest.gadget.id`.

### Plugin manifests

- `plugins/bangs/manifest.toml`, `plugins/calculator/manifest.toml`, `plugins/emoji-picker/manifest.toml`, `plugins/hello-world/manifest.toml`, `plugins/open-url/manifest.toml`, `plugins/template/manifest.toml`, `plugins/zerotier/manifest.toml`: line 1 `[plugin]` → `[gadget]`.

### Test fixtures

- `src-tauri/tests/fixtures/{assets,command,failing-enable,minimal,opener-http,website-metadata}-plugin/manifest.toml`: line 1 `[plugin]` → `[gadget]`. Directory names stay until P5.

### Documentation strings

- WIT prose inside `torchsnap-gadget.wit` referencing `manifest.toml`'s `[plugin]`/`[storage.sql]`/`[permissions.opener]` etc.: only `[plugin]` references rename. Sub-tables (`[permissions.*]`, `[storage.*]`, `[[tasks]]`) are unchanged — they were keys *under* `[plugin]` and remain keys *under* `[gadget]`; their bare names don't include "plugin".

## Commit grouping

P4 lands as a sequence of atomic, semantically grouped commits, each of which compiles + tests cleanly:

1. **`rename: WIT package, world, file (plugin → gadget)`** — git mv of `.wit`; package + world rename; `wit_bindgen::generate!` args; SDK `pub use` paths in `lib.rs` and `sql.rs`. The SDK crate name and the `define_plugin!` macro name are unchanged in this commit; plugin crates still depend on `torchsnap-plugin-sdk` and import via `torchsnap_plugin_sdk::prelude::*`. Those imports resolve through the SDK's re-exports, which post-commit point at the new generated `torchsnap::gadget::*` modules. The commit is atomic and compiles cleanly from `plugins/`.

2. **`rename: Rust SDK crate (torchsnap-plugin-sdk → torchsnap-gadget-sdk) + define_plugin! → define_gadget!`** — SDK `Cargo.toml` `name`, `wit_bindgen!` `default_bindings_module`, all 7 plugin crate `Cargo.toml` files, all `use torchsnap_plugin_sdk::…` → `use torchsnap_gadget_sdk::…`, every `define_plugin!(…)` → `define_gadget!(…)`, SDK rustdoc and macro definition rename. Verify with `cargo build --release` from `plugins/`.

3. **`rename: Tauri command names (wasm_plugins, plugin_sources, plugin_message, install_plugin_archive, uninstall_user_plugin)`** — Rust `#[tauri::command]` function names + `generate_handler!` registration + TS `CommandMap` keys + every `command("…", …)` call site. Verify with `just check` and `bun typecheck`.

4. **`rename: Tauri event names and payload keys (open-plugin-settings, activate-plugin-custom-ui, pluginId → gadget*)`** — Touches `plugin_host.rs:73–80, 743–746, 980–989` and `src/launcher/Launcher.tsx:232–244`. Both event names and both payload keys. Verify with manual launcher test (custom-UI shortcut activates and settings navigation works).

5. **`rename: settings-store key prefix (plugins.<id>.* → gadgets.<id>.*)`** — `plugin_host.rs` (4 sites + delete migration block at lines 206–215) + `plugin_install.rs` (impl + test helper + 4 test bodies) + WIT prose. Verify with `cargo test -p torchsnap`.

6. **`rename: permission-var tokens (${plugin-data} → ${gadget-data}, ${plugin-archive} → ${gadget-archive})`** — `permission_vars.rs` (`RECOGNIZED_PERMISSION_VARIABLES`, `PathContext` field names, `lookup` match arms, rustdoc, unit tests) + `manifest/permissions/command.rs` test fixtures + `runtime/host/fs.rs` test helper + all `PathContext` construction sites + any plugin/fixture manifests using these tokens. Verify with `cargo test -p torchsnap` and confirm `validate_variable_references` rejects the old token names.

7. **`rename: source-discriminator wire strings (ResultSource + LogSource)`** — Touches `commands/types.rs:309–346`, `wasm/logging/mod.rs:88–95, 257–270`, `src/types.ts:120`, `src/devtools/types.ts:17`, plus 5 TS devtools-console comparators. Verify with `cargo test` (serde round-trips) and `bun typecheck`.

8. **`rename: bundled.toml plugins → gadgets + tools parser`** — `plugins/bundled.toml` body, `tools/list-bundled-plugins` parser line 50 + diagnostic strings. Verify with `just stage-bundled-plugins` (recipe still named `stage-bundled-plugins`; renamed to `stage-bundled-gadgets` in P5).

9. **`rename: npm @torchsnap/plugin-sdk → @torchsnap/gadget-sdk`** — `packages/plugin-sdk/package.json` name + 5 plugin frontend `package.json` deps + every `@torchsnap/plugin-sdk[/...]` import in `src/`, plugin frontends, and SDK shim files (jsdoc + Vite plugin name string + 4 runtime error strings + `initPluginSdk` → `initGadgetSdk` substitution inside the renamed string). Regenerate `bun.lock` (or equivalent) in same commit. Verify with `bun typecheck` and `bun test` and a full launcher build.

10. **`rename: manifest.toml [plugin] → [gadget] section`** — Rust parser field rename (`Manifest.plugin` → `Manifest.gadget`) + 20+ inline test fixtures + 7 plugin manifests + 6 test-fixture manifests. Verify with `cargo test -p torchsnap` (manifest parser tests).

Commits 1–2 must come first (they are the SDK foundation). Commits 3–9 are independent of each other; group them per the order above for log readability. Commit 10 is independent but lands last because it changes serde input shape and any incomplete coverage would surface as a parser failure on launch.

## Test impact

### Rust tests

- `src-tauri/src/wasm/manifest/mod.rs`: 20+ unit tests with embedded `[plugin]` headings — every fixture-string heading and every assertion `manifest.plugin.…` is updated.
- `src-tauri/src/commands/types.rs:316–346`: 3 `result_source_tests` — assertion strings `"plugin"` → `"gadget"`. The "plugin id round-trips special characters" test name itself is internal; rename to `gadget_id_round_trips_special_characters` for clarity.
- `src-tauri/src/wasm/logging/mod.rs:253–270` (and around): `LogSource` serde round-trip test at line 257 asserts `"plugin"` shape — flip to `"gadget"`.
- `src-tauri/src/wasm/permission_vars.rs`: unit tests that use the literal token strings `"plugin-data"` / `"plugin-archive"` — update to `"gadget-data"` / `"gadget-archive"`. The `ctx()` helper field names update from `plugin_data` / `plugin_archive` to `gadget_data` / `gadget_archive`. The `validate_rejects_first_unknown_variable_in_chain` test at line 278 uses `"${plugin-data}/${unknown}/${home}"` — change to `"${gadget-data}/${unknown}/${home}"` so it still tests a valid token alongside an unknown one.
- `src-tauri/src/wasm/manifest/permissions/command.rs`: `accept_recognized_variables_in_literal` test at lines 535–550 iterates `"plugin-data"` and `"plugin-archive"` — update both entries to `"gadget-data"` / `"gadget-archive"`.
- `src-tauri/src/wasm/runtime/host/fs.rs`: `ctx_for` test helper — field names and `root.join(...)` strings update as described in the plan section above.
- `src-tauri/src/plugin_install.rs:284–390`: `tests` module — `keys_to_strip_for` helper + 5 tests use literal `"plugins.foo.alpha"`, `"plugins.foo.beta"`, `"plugins.foo.nested.key"`, `"plugins.calc.shortcut"`, `"plugins.calculator.history"`, `"plugins.calculator.enabled"`, `"plugins.other-plugin.key"`. All update to `"gadgets.…"`. The regression test name `cleanup_ignores_other_plugins_with_longer_ids` and similar are internal; renaming for clarity is optional.
- Integration tests under `src-tauri/tests/`: confirm none assert against the literal command names or the `plugins.<id>.` prefix or the `[plugin]` section name. If any do, update.
- Add a **new round-trip serde test** for both `ResultSource` and `LogSource` after rename, locking in the wire-string `"gadget"` so future regressions surface immediately. Place alongside existing `result_source_tests` and `LogSource` tests.

### TS tests

- `bun test` — confirm `packages/plugin-sdk/`'s mock context tests use the renamed package name. Update any test that imports from `@torchsnap/plugin-sdk[/testing]`.
- Add a regression test (or update existing) for the `command()` wrapper that pins down the renamed command names. Optional but advisable given how many call sites change.

### Verification of serde round-trip across the bridge

Per the cross-cutting note "tests and docs coverage is mandatory", the rename of `ResultSource` and `LogSource` discriminators is the highest-risk wire change. Land an explicit round-trip test that reads `serde_json::to_value(&Source::Gadget(...))` and asserts `{"type":"gadget", …}` exactly, as a regression guard.

## Documentation impact

P4 touches doc strings *inside files that P4 already renames*:

- All rustdoc inside `plugins/plugin-sdk/src/lib.rs`, `command.rs`, `sql.rs`, `website_metadata.rs`, `messaging.rs`, `settings.rs`, `logging.rs` referencing `torchsnap_plugin_sdk` / `define_plugin!` / `[plugin]` — updates ride along.
- All jsdoc inside `packages/plugin-sdk/src/**/*.{ts,tsx}` referencing `@torchsnap/plugin-sdk` / `initPluginSdk` / "plugin bundles" — update inside the literals being touched.
- Rustdoc inside `src-tauri/src/wasm/manifest/mod.rs` describing the `[plugin]` section — references update.
- Rustdoc inside `src-tauri/src/plugin_host.rs`, `src-tauri/src/plugin_install.rs` describing `plugins.<id>.…` — references update.
- `torchsnap-gadget.wit` prose (the entire file is rewritten as part of step 1).

P4 does **not** touch:

- `docs/Plugin-Architecture/**` (P2).
- ADRs (P2).
- README / CLAUDE.md (P2).

## Verification steps

1. **WIT toolchain**: `just check-wit` and `just fmt-wit` against the renamed `wit/torchsnap-gadget.wit`. `wasm-tools` reads the file by extension; the rename is transparent.
2. **WASM gadget builds**: `cd plugins && cargo build --release` (the workspace default `wasm32-wasip2`). Each of the 7 gadgets compiles. The SDK crate compiles. The `wit_bindgen::generate!` macro produces `torchsnap::gadget::*` modules and the SDK re-exports resolve cleanly.
3. **Host builds + tests**: `just check`, `just test`, `just lint`. Run `cargo test -p torchsnap` to specifically exercise manifest parser, settings-key tests, serde round-trip tests, and permission-var substitution tests. Confirm `validate_variable_references` rejects `${plugin-data}` and accepts `${gadget-data}`.
4. **TS pipeline**: `bun typecheck`, `bun test`, `bun lint`. Run `bun install` first so the renamed `@torchsnap/gadget-sdk` resolves through the `file:` link.
5. **Manual launcher run**: `just dev`. Verify (a) launcher opens, (b) gadget shortcuts trigger custom-UI activation (validates `activate-gadget-custom-ui` event), (c) "Open settings for this gadget" action navigates correctly (validates `open-gadget-settings` event), (d) settings panel shows gadgets, (e) install/uninstall a `.torchsnap` archive end-to-end (validates `install_gadget_archive` + `uninstall_user_gadget`), (f) per-gadget settings persist across restart (validates `gadgets.<id>.<key>` storage prefix).
6. **Storage clean-state validation**: per the no-migration decision, before the first launch on the renamed code, manually `rm -rf <app_data_dir>/{plugins,plugin-home}` (or whatever exists). After first launch with the renamed code, the host writes only `gadgets.<id>.…` keys; no `plugins.<id>.…` keys appear. Confirm via the Tauri Store devtools or a direct read of `<app_data_dir>/settings.json`.
7. **Bundled-archive path**: `just build` runs `stage-bundled-plugins` (recipe name unchanged in P4). Confirm `target/bundled-plugins/` (P5 will rename) contains 5 `.torchsnap` archives, each from the renamed gadget crates with new manifest section names.
8. **Devtools log stream**: open the devtools console; verify log entries from gadgets show source as `{ type: "gadget", value: "<id>" }` (LogSource) and search-result entries show `{ type: "gadget", id: "<id>" }` (ResultSource).

## Risks & rollback

### Risks

- **Wire-format desync.** If any one of the 5 Tauri commands or 2 event names is renamed on only one side of the IPC boundary, the launcher silently misbehaves (no error message, just dead UI). Mitigation: per-commit verification with `bun typecheck` (the `CommandMap` is exhaustive at the type level — a missing/mismatched key fails compilation). For event names, the type system does not catch mismatches; manual launcher test (step 5 above) is the gate.
- **Settings persistence loss.** Existing test installs that haven't manually cleared `<app_data>/{plugins,plugin-home}` will lose all per-gadget settings on first launch (because the keys move from `plugins.<id>.…` to `gadgets.<id>.…`). Per the locked decision this is *expected*, not a regression. Document in commit message and verification step 6.
- **WIT package coupling**. The triple coupling (WIT `package`, WIT `world`, `wit_bindgen!` `world` arg, `default_bindings_module`, SDK `Cargo.toml name`, SDK `pub use` paths) is fragile. A rename that gets only 5 of the 6 right yields a slow, non-obvious cargo error. Mitigation: commit 1 and commit 2 are validated by a clean `cargo build --release` from `plugins/` before commit is made.
- **Downstream lockfile churn.** `bun.lock` (or equivalent) is regenerated when the package name changes. If the lockfile is committed, regenerate it in the same commit. If not committed, no risk.
- **Manifest field rename + serde rename collisions.** The Rust struct field `plugin` becomes `gadget`. If any other code reads the field by reflection (none expected, but check for `serde_value`/`toml::Value` accesses), update those too.
- **Test fixtures spread across two locations.** Manifest test fixtures live in two places: (a) inline in `src-tauri/src/wasm/manifest/mod.rs` test module, (b) on-disk under `src-tauri/tests/fixtures/`. Both move in lockstep in commit 10.
- **`define_plugin!` re-export through prelude.** The macro is re-exported as `pub use super::{define_plugin, …}`. Renaming requires updating both the `macro_rules!` definition and the `pub use` line — easy to miss one. Mitigation: a single `cargo build --release` from `plugins/` after commit 2 surfaces the error immediately because all 7 plugins call `define_gadget!(...)`.

### Rollback

- Each commit is atomic and reverts cleanly with `git revert`.
- If a partial-rename slips into main, follow-up by re-running the failing rename pass; the wire-format incompatibility is contained to the single mismatched item.
- **Migration-block deletion** (lines 206–215 of `plugin_host.rs`): reviewers should notice this deletion. On rollback, the block is restored by `git revert`. There is no data-safety issue because the no-migration decision means test storage is cleared before P4 lands.
- **No data migration burden** if rolling back: existing `<app_data>/{plugins,plugin-home}` are untouched on revert (they were already cleared by tester before P4 launched per decision).

---

### Critical Files for Implementation

- `/Users/jakob/Development/github/jakobwesthoff/torchsnap/plugins/plugin-sdk/wit/torchsnap-plugin.wit`
- `/Users/jakob/Development/github/jakobwesthoff/torchsnap/plugins/plugin-sdk/src/lib.rs`
- `/Users/jakob/Development/github/jakobwesthoff/torchsnap/src-tauri/src/wasm/manifest/mod.rs`
- `/Users/jakob/Development/github/jakobwesthoff/torchsnap/src-tauri/src/plugin_host.rs`
- `/Users/jakob/Development/github/jakobwesthoff/torchsnap/src/lib/command.ts`

---

**Summary.** This Phase 4 plan covers every wire-surface boundary: the WIT package/world/file rename and its triple-coupled `wit_bindgen!`/Cargo/`pub use` ripple; the Rust SDK crate rename including `define_plugin!` → `define_gadget!` across all 7 gadget crates; the npm package rename including all 4 shim runtime-error strings, the Vite plugin name, the 5 frontend `package.json`s, and every `@torchsnap/plugin-sdk[/...]` deep import; all 5 Tauri command names with their TS registry; both Tauri event names (`open-plugin-settings`, `activate-plugin-custom-ui`) and their payload key `pluginId`; the settings-key prefix with deletion of the legacy migration fallback at lines 206–215 of `plugin_host.rs`; the manifest permission-var tokens `${plugin-data}` and `${plugin-archive}` across `permission_vars.rs`, the manifest parser, the fs host import, and all fixture manifests; both `ResultSource` and `LogSource` wire discriminators; the `bundled.toml` key plus its TS parser in `tools/list-bundled-plugins`; and the `[plugin]` → `[gadget]` manifest section name across the parser, 7 gadget manifests, 6 fixture manifests, and 20+ inline test fixtures. The plan groups these into 10 atomic commits with explicit verification per commit, defers all directory/path/recipe-name renames to P5, and respects the locked-in "no migration code" decision by deleting (not renaming) the legacy migration fallback. No open questions remain.