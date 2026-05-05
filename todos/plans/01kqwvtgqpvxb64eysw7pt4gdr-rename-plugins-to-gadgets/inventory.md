# Plugins → Gadgets Rename: Shared Inventory

This file is the *shared* inventory consulted by all five phase plans
in this directory. It is **not** a plan; it captures the raw scope
data so that each phase plan can be authored without re-running the
same searches.

## Decision context (locked in by user)

- **Pre-alpha**: torchsnap is the only consumer of its own APIs.
  No version-bump strategy, no deprecation period — straight rename.
- **No storage migration code**: existing test installations will
  have their `<app_data>/plugins/` and `<app_data>/plugin-home/`
  trees deleted manually before first launch on the renamed code.
  Plans must NOT include migration code.
- **Manifest filename stays**: `manifest.toml` remains the filename.
  Inner keys/sections still rename (e.g. `[plugin]` → `[gadget]`).
- **Marketing site is out of scope.** Only this repository.
- **Existing ADRs are amended/superseded** by one new ADR. The new
  ADR cross-links to all touched ADRs via the `adrs` tool (run with
  `EDITOR=true` per project convention).

## Naming map (authoritative — every plan must follow this)

| Old | New |
| --- | --- |
| Plugin (user-facing label) | Gadget |
| `plugin` / `plugins` (TS/Rust ident) | `gadget` / `gadgets` |
| `Plugin*` types (Rust/TS) | `Gadget*` |
| `PluginHost` | `GadgetHost` |
| `PluginSlot` | `GadgetSlot` |
| `PluginSource` (trait) | `GadgetSource` |
| `PluginSourceKind` | `GadgetSourceKind` |
| `PluginSourceRegistry` | `GadgetSourceRegistry` |
| `PluginState` (wasm runtime) | `GadgetState` |
| `PluginId` | `GadgetId` |
| `PluginFrecency` | `GadgetFrecency` |
| `WasmPluginBridge` | `WasmGadgetBridge` |
| `WasmPluginManifest` | `WasmGadgetManifest` |
| `InstalledPluginInfo` | `InstalledGadgetInfo` |
| `define_plugin!` macro | `define_gadget!` |
| `impl_noop_*!` macros | unchanged (no `plugin` token) |
| Rust trait `Plugin` (host built-ins) | `Gadget` |
| `*Plugin` host built-in struct names (`AppLauncherPlugin`, `BuiltInCommandsPlugin`, `ClipboardPlugin`, `SystemCommandsPlugin`, `SystemPreferencesPlugin`, `MockPlugin`) | `*Gadget` (`AppLauncherGadget`, etc.) |
| Plugin crate names (`*-plugin`) | `*-gadget` (`bangs-gadget`, `calculator-gadget`, `emoji-picker-gadget`, `hello-world-gadget`, `open-url-gadget`, `template-gadget`, `zerotier-gadget`) — confirm via plugin Cargo.toml inspection in P5 |
| Per-plugin struct names in plugin crates (`ZeroTierPlugin`, `BangsPlugin`, `EmojiPickerPlugin`, `CalculatorPlugin`, `TemplatePlugin`, `OpenUrlPlugin`, `HelloWorld`) | `ZeroTierGadget`, `BangsGadget`, `EmojiPickerGadget`, `CalculatorGadget`, `TemplateGadget`, `OpenUrlGadget`, `HelloWorld` (no Plugin in name — keep) |
| `PluginContext` / `PluginContextProvider` / `PluginContextValue` | `GadgetContext` / `GadgetContextProvider` / `GadgetContextValue` |
| `PluginInfo` / `PluginRuntime` / `PluginSendMessage` | `GadgetInfo` / `GadgetRuntime` / `GadgetSendMessage` |
| `PluginViewProps` / `PluginSettingsProps` | `GadgetViewProps` / `GadgetSettingsProps` |
| `usePluginInfo` / `usePluginRuntime` / `usePluginSetting` / `usePluginStream` / `UsePluginSetting` | `useGadgetInfo` / `useGadgetRuntime` / `useGadgetSetting` / `useGadgetStream` / `UseGadgetSetting` |
| `MockPluginContextProvider` (+ Props) | `MockGadgetContextProvider` |
| `pluginColors` / `PLUGIN_COLORS` / `pluginColorIndex` | `gadgetColors` / `GADGET_COLORS` / `gadgetColorIndex` |
| `initPluginSdk` | `initGadgetSdk` |
| `sendPluginMessage` | `sendGadgetMessage` |
| `injectPluginCss` / `removePluginCss` | `injectGadgetCss` / `removeGadgetCss` |
| `pluginComponent` (factory in `src/lib/pluginComponent.tsx`) | `gadgetComponent` |
| `pluginId` JS variable / param | `gadgetId` |
| `data-plugin` / `data-plugin-css` HTML attributes | `data-gadget` / `data-gadget-css` |
| Tauri command `wasm_plugins` | `wasm_gadgets` |
| Tauri command `plugin_sources` | `gadget_sources` |
| Tauri command `plugin_message` | `gadget_message` |
| Tauri command `install_plugin_archive` | `install_gadget_archive` |
| Tauri command `uninstall_user_plugin` | `uninstall_user_gadget` |
| Tauri event name `open-plugin-settings` | `open-gadget-settings` |
| Tauri event name `activate-plugin-custom-ui` | `activate-gadget-custom-ui` |
| Tauri event payload key `pluginId` | `gadgetId` |
| Settings key prefix `plugins.<id>.<key>` | `gadgets.<id>.<key>` |
| Permission var token `${plugin-data}` (manifest permission contract) | `${gadget-data}` |
| Permission var token `${plugin-archive}` (manifest permission contract) | `${gadget-archive}` |
| Source-discriminator value `{"type":"plugin","id":...}` (devtools console source format) | `{"type":"gadget","id":...}` |
| WIT package `torchsnap:plugin@0.1.0` | `torchsnap:gadget@0.1.0` |
| WIT world `plugin` | `gadget` |
| WIT file `plugins/plugin-sdk/wit/torchsnap-plugin.wit` | `gadgets/gadget-sdk/wit/torchsnap-gadget.wit` |
| URI scheme `torchsnap-plugin://` | `torchsnap-gadget://` |
| Manifest section `[plugin]` | `[gadget]` |
| `bundled.toml` key `plugins = [...]` | `gadgets = [...]` |
| App-data dir `<app_data>/plugins/` | `<app_data>/gadgets/` |
| App-data dir `<app_data>/plugin-home/<id>/` | `<app_data>/gadget-home/<id>/` |
| Workspace dir `plugins/` | `gadgets/` |
| Workspace dir `plugins/plugin-sdk/` | `gadgets/gadget-sdk/` |
| npm package dir `packages/plugin-sdk/` | `packages/gadget-sdk/` |
| npm package name `@torchsnap/plugin-sdk` | `@torchsnap/gadget-sdk` |
| Rust crate `torchsnap-plugin-sdk` | `torchsnap-gadget-sdk` |
| Module path `torchsnap_plugin_sdk` | `torchsnap_gadget_sdk` |
| Vite plugin name string `"torchsnap-plugin-sdk"` | `"torchsnap-gadget-sdk"` |
| Just recipe file `just/plugins.just` | `just/gadgets.just` |
| Just recipes invoking `build-plugin`, `stage-bundled-plugins`, etc. | `build-gadget`, `stage-bundled-gadgets`, etc. |
| Bundle staging dir `target/bundled-plugins/` | `target/bundled-gadgets/` |
| Tauri resources path in `tauri.conf.json` | corresponding update |
| Archive extension `.torchsnap` | **unchanged** (product name, not plugin) |
| Todos tree `todos/plugin-host/...` | `todos/gadget-host/...` |
| Todos tree `todos/plugins/...` | `todos/gadgets/...` |
| Doc tree `docs/Plugin-Architecture/...` | `docs/Gadget-Architecture/...` |

**Vite peer-dep `@tauri-apps/plugin-store`** is an external Tauri
plugin and is NOT in scope. Do not rename `tauri-plugin-store`,
`@tauri-apps/plugin-*`, or anything from the Tauri ecosystem.

**`torchsnap-plugin-sdk` Vite plugin's `Plugin` type import** comes
from `vite` (`import type { Plugin } from "vite"`). That is Vite's
own `Plugin` type and stays.

## Counts (for sizing)

- `*.rs` matches: ~2178 lines containing `plugin|Plugin` across
  `src-tauri/` + `plugins/`.
- `*.ts` / `*.tsx` matches: dozens of files across `src/`,
  `packages/plugin-sdk/`, plugin frontends.
- ADRs touching plugin terminology: ~25 (see list below).
- Plugin-Architecture docs: 6 files (`docs/Plugin-Architecture/01-overview.md`
  through `06-settings-reactivity.md`).
- `manifest.toml` files: 6 plugin manifests + 5 test fixture
  manifests under `src-tauri/tests/fixtures/`.

## Phase boundaries (locked in)

### Phase 1 — App UI strings
User-facing copy in TSX. No symbol renames, no API changes.
Touches: settings sidebar, plugins management panel, devtools
console headers/empty states, launcher-related copy that mentions
"plugin". Keep symbol names alone — those belong to P3.

### Phase 2 — Documentation + ADR
- New ADR consolidating the rename decision, cross-linked to all
  touched ADRs via `EDITOR=true adrs link`.
- Rewrite `docs/Plugin-Architecture/` (rename folder to
  `docs/Gadget-Architecture/`, update content).
- Rewrite `docs/api/plugin-development.md` (rename + update).
- Rewrite `docs/strategy/Selfcontained-Plugin-System.md`.
- Update `docs/research/wasm-wit-plugin-system.md`.
- Update `docs/control-api.md`, `docs/Howto-build-on-fedora-43.md`,
  `docs/api/logging-system.md` if they reference plugins.
- Existing ADRs: do NOT rewrite history. Add a *brief amendment
  banner* at the top of each touched ADR pointing at the new ADR;
  `adrs link` handles the bidirectional links.
- README + CLAUDE.md (root + project-level): rewrite plugin
  terminology to gadget. Global `~/.claude/CLAUDE.md` is OUT OF
  scope (user's private global).
- Todos tree: rename `todos/plugin-host/` → `todos/gadget-host/`,
  rename `todos/plugins/` → `todos/gadgets/`, scan every file under
  `todos/` for plugin terminology and update content. (Use
  `git mv`.)

### Phase 3 — Internal code rename (non-public)
Rust host (`src-tauri/`) and TS internal modules (`src/`) — every
symbol, file, and module that does NOT cross the public-API
boundary or the wire boundary. Tauri command names stay as-is in
this phase (they're handled in P4 because they're a wire surface).
Internal-only Rust types (`PluginHost`, `PluginSlot`, host built-in
`Plugin` trait & impls, `PluginSource` trait, `PluginState`,
`PluginId`, `PluginFrecency`, etc.). Internal-only TS types
(`PluginContext*` family, hooks, `pluginCss.ts` etc.) — rename
file, then symbols. URI scheme `torchsnap-plugin://` is internal
between host and host-loaded code; rename it here, both Rust and TS
sides. TSX filename renames `PluginsManagementPanel.tsx` →
`GadgetsManagementPanel.tsx` and `PluginSettingsWrapper.tsx` →
`GadgetSettingsWrapper.tsx` ride P3 with their symbol renames
(per user decision). Permission var tokens (`${plugin-data}` and
`${plugin-archive}`) are NOT in P3 — they live in P4 alongside
other manifest/wire surfaces.

### Phase 4 — Public/wire API surface
Anything visible on the wire to plugin authors or to plugin
guests:
- WIT (`torchsnap:plugin@0.1.0` → `torchsnap:gadget@0.1.0`,
  world rename, file rename).
- `define_plugin!` macro rename.
- Rust SDK crate `torchsnap-plugin-sdk` (name + module path).
- npm `@torchsnap/plugin-sdk` (name, exports map, all
  `@torchsnap/plugin-sdk/*` deep-import paths in shims, vite,
  testing).
- Tauri command names (`wasm_plugins`, `plugin_sources`,
  `plugin_message`, `install_plugin_archive`,
  `uninstall_user_plugin`).
- Tauri event names (`open-plugin-settings` → `open-gadget-settings`,
  `activate-plugin-custom-ui` → `activate-gadget-custom-ui`).
- Tauri event payload keys (`pluginId` → `gadgetId`).
- Settings-store key prefix (`plugins.<id>.` → `gadgets.<id>.`).
- Manifest permission-var tokens (`${plugin-data}` → `${gadget-data}`,
  `${plugin-archive}` → `${gadget-archive}`). These are author-facing
  manifest contract surfaces (e.g. `path-under = "${plugin-data}/foo"`),
  so both renames live with the rest of the wire/manifest surface in P4
  rather than the internal-only P3 set.
- Source discriminator wire string `{"type":"plugin"...}`.
- `bundled.toml` key (`plugins = [...]` → `gadgets = [...]`).
- `manifest.toml` section name `[plugin]` → `[gadget]`.

### Phase 5 — Repo layout, tooling, storage paths
- `plugins/` → `gadgets/`, `plugins/plugin-sdk/` →
  `gadgets/gadget-sdk/`, `packages/plugin-sdk/` →
  `packages/gadget-sdk/`. All `git mv`. Cargo.toml workspace
  members, `package.json` workspaces, all imports get updated.
- Test fixtures: `src-tauri/tests/fixtures/*-plugin/` →
  `*-gadget/`.
- Just recipes: rename `just/plugins.just` → `just/gadgets.just`,
  rename every recipe (`build-plugin`, `stage-bundled-plugins`,
  `package-plugin`, etc.) and every cross-reference in other
  `.just` files (`bangs.just`, `build.just`, etc.).
- `target/bundled-plugins/` → `target/bundled-gadgets/`. Update
  `.gitignore` and `tauri.conf.json` resources.
- App-data paths: `<app_data>/plugins/` → `<app_data>/gadgets/`,
  `<app_data>/plugin-home/<id>/` → `<app_data>/gadget-home/<id>/`.
  No migration code (per decision); update path string literals
  and surrounding doc comments.
- Per-plugin Cargo.toml package names (`*-plugin` → `*-gadget`)
  and lib names. (Plan should confirm by reading each.)
- vite.config.ts entries that reference `plugins/` paths.

## Source pointers (to save plan agents redundant searches)

### Rust host

- `src-tauri/src/plugin_host.rs` — `PluginHost` (216 hits)
- `src-tauri/src/plugins/` — host built-in trait `Plugin` and
  impls (mod.rs, app_launcher.rs, clipboard/, commands.rs,
  system_commands/, system_preferences.rs)
- `src-tauri/src/plugin_install.rs` — install/uninstall commands +
  storage path strings
- `src-tauri/src/wasm/` — runtime, manifest, source, protocol,
  bridge, bindings, discovery, logging
- `src-tauri/src/lib.rs:537–562` — Tauri commands `wasm_plugins`,
  `plugin_sources`
- `src-tauri/src/commands/mod.rs:73` — `plugin_message`
- `src-tauri/src/wasm/protocol.rs` — URI scheme registration
- `src-tauri/src/wasm/bindings.rs:247,315,547,585,591` — URI
  scheme construction
- `src-tauri/src/wasm/permission_vars.rs:46–47` — `"plugin-data"` and
  `"plugin-archive"` tokens (P4 — wire/manifest contract)
- `src-tauri/src/frecency/plugin_frecency.rs` — `PluginFrecency`
- `src-tauri/src/frecency/mod.rs` — references plugin frecency
- `src-tauri/src/commands/types.rs:326` — wire-format test for
  `{"type":"plugin","id":"bangs"}`
- `src-tauri/src/plugin_host.rs:198,210,241,411,845` — settings
  key prefix `plugins.<id>.`
- `src-tauri/src/plugin_install.rs:255,306` — same prefix
- `src-tauri/src/plugin_host.rs:743–745` — Tauri event with
  `"pluginId"` payload key

### Frontend (TS/TSX)

- `src/lib/sdk.ts` — `initPluginSdk`
- `src/lib/pluginCss.ts` — `injectPluginCss`, `removePluginCss`,
  URI scheme construction, `data-plugin` attributes
- `src/lib/pluginMessage.ts` — `sendPluginMessage`, calls
  `command("plugin_message", ...)`
- `src/lib/pluginComponent.tsx` — `pluginComponent` factory
- `src/lib/command.ts` — Tauri command registry types
  (`plugin_message`, `wasm_plugins`, `plugin_sources`,
  `install_plugin_archive`, `uninstall_user_plugin`)
- `src/contexts/PluginContext.tsx` (+ `PluginContextProvider`,
  `usePluginInfo`, `usePluginRuntime`, `usePluginSetting`,
  `index.ts`)
- `src/hooks/usePluginStream.ts`
- `src/launcher/Launcher.tsx`, `LauncherFooter.tsx`,
  `main.tsx`, `visibility.ts`, `hooks/useSearch.ts`,
  `hooks/useKeyboardNavigation.ts`
- `src/settings/PluginSettingsWrapper.tsx`,
  `SettingsSidebar.tsx`, `SettingsPanel.tsx`,
  `sections/PluginsManagementPanel.tsx`,
  `sections/GeneralSection.tsx`,
  `sections/FrecencySection.tsx`,
  `sections/WebsiteMetadataSection.tsx`, `main.tsx`
- `src/devtools/console/formatters.ts` (source discriminator),
  `ConsoleToolbar.tsx`, `LogList.tsx`, `pluginColors.ts` (file
  rename)
- `src/plugins/types.ts`
- `src/lib/logger.ts` — comment references
- `src/settingsStore.ts` — only references `tauri-plugin-store`
  (out of scope external)
- `src/contexts/ThemeProvider.tsx:95` — internal comment
  reference

### `packages/plugin-sdk/`

- `package.json` — name + exports paths
- `src/shims/{components,hooks,jsx-runtime,keybindings,react,utils}.ts`
- `src/testing/{index.ts,MockPluginContextProvider.tsx,setup.ts}`
- `src/types/{index.ts,plugin.ts,settings.ts,logger.ts,data.ts}`
- `src/vite/index.ts` — Vite plugin name string
- `theme.css` (export only — not necessarily renamed content)

### Plugin crates (`plugins/`)

- `plugins/plugin-sdk/` — Cargo.toml, src/lib.rs, wit/torchsnap-plugin.wit
- `plugins/{bangs,calculator,emoji-picker,hello-world,open-url,template,zerotier}/`
  — Cargo.toml, manifest.toml, src/lib.rs, frontend/ where present
- `plugins/Cargo.toml` — workspace root
- `plugins/.cargo/config.toml` — wasip2 default target
- `plugins/bundled.toml`
- `plugins/zerotier/` has the largest secondary footprint
  (api/, auth.rs, history.rs, query.rs, actions.rs)

### Test fixtures

- `src-tauri/tests/fixtures/{assets,command,failing-enable,minimal,opener-http,website-metadata}-plugin/`
  — directory rename + manifest.toml + Cargo.toml updates

### Just / build

- `Justfile` (root)
- `just/{assets,bangs,build,devcontainer,doctor,install,maintenance,plugins,quality,start,tools}.just`
  — `plugins.just` is the primary target; cross-references in the
  others
- `tauri.conf.json` — `resources` entry pointing at
  `target/bundled-plugins/`
- `src-tauri/Cargo.toml` — references to plugin SDK if any
- `vite.config.ts` (root) — references to plugin SDK
- `plugins/<id>/frontend/vite.config.ts` (where present)

### Docs

- `docs/Plugin-Architecture/01-overview.md` …
  `06-settings-reactivity.md`
- `docs/api/plugin-development.md`,
  `docs/api/logging-system.md`
- `docs/strategy/Selfcontained-Plugin-System.md`
- `docs/research/wasm-wit-plugin-system.md`
- `docs/control-api.md`
- `docs/Howto-build-on-fedora-43.md`
- ADRs touching plugin terminology (full list follows; not all
  need new content — most just need an amendment banner pointing
  at the new ADR):
  - 0008, 0010, 0011, 0012, 0013, 0014, 0015, 0016, 0017,
    0018, 0019, 0021, 0022, 0023, 0024, 0025, 0027, 0028,
    0029, 0030, 0031, 0032, 0033, 0034, 0035, 0036, 0037,
    0038, 0039, 0040, 0041

### Todos

- `todos/plugin-host/` — `wasm/`, `audit/`, `sdk/`, `api/`
  subtrees
- `todos/plugins/` — per-plugin todos
  (`bangs/`, `calculator/`, `clipboard/`, `future/`,
  `open-url/`, `zerotier/`)
- `todos/plans/` — this directory itself; the ORIGINAL trigger
  todo lives at
  `todos/01kqw4kqe3g50xn77efwv71jy2-rename-plugins-to-gadgets-everywhere.md`
  and should be updated (or removed) when phases are realized,
  not now.

## Cross-cutting notes for plan authors

1. **Tests and docs coverage is mandatory** in every plan
   (project rule). Each phase plan must have explicit sub-sections
   covering: existing test updates, new tests where the rename
   creates a behavioural risk, and doc updates owned by the
   phase. Mechanical-rename phases still need doc updates for
   any docstrings or rustdoc/jsdoc that mention the renamed
   symbols.
2. **Verification after rename**: each plan ends with a
   build/test verification step:
   - Rust host: `just check` / `just test` / `just lint`
   - WASM plugins: per-plugin `cargo build --release` from
     the virtual workspace
   - TS: `bun typecheck` / `bun test` / `bun lint`
   - WIT: `just check-wit` / `just fmt-wit`
3. **Use `git mv`** for tracked file moves (project rule).
4. **Atomic commits** grouped by semantic feature. Within a
   phase, expect multiple commits (e.g. one per major
   subsystem).
5. **No CI changes** unless a phase explicitly affects CI
   (P5 might, due to `target/bundled-plugins/`).
6. **MPL-2.0 headers** stay on every renamed file (project
   rule). Do not strip them during a rename.
7. **Dependency order between phases**: P1 is independent of
   P3/P4. P2 depends on naming choices being final, but its
   prose can land before code rename. P3 must land before P4
   touches Tauri command names (because P4 will assume P3's
   Rust types exist with their new names). P5 lands last
   because the directory rename invalidates relative paths
   referenced by every other phase. Each plan should state its
   own assumed predecessors.
8. **Where ambiguity exists**: each plan must list open
   questions at the top under an "Open questions" heading
   rather than guessing. Do not silently invent answers.
