# Phase 5 — Repo Layout, Tooling, Storage Paths

## Summary

Phase 5 is the final, paths-only phase of the Plugin → Gadget rename. It moves three directory trees (`plugins/`, `plugins/plugin-sdk/`, `packages/plugin-sdk/`), renames seven test-fixture directories, renames every per-plugin Cargo crate (`*-plugin` → `*-gadget`) plus the resulting WASM artifact filenames, renames `just/plugins.just` and every recipe inside it, renames the Tauri bundle staging dir `target/bundled-plugins/` → `target/bundled-gadgets/`, and updates every relative path that references any of the above (Cargo workspace, npm workspace deps, Justfiles, Tauri config, devcontainer, gitignore, host-side `wit_bindgen` path, host-side rustdoc, and a handful of app-data string literals like `<app_data>/plugins/` and `<app_data>/plugin-home/`). No migration code is added — the user manually deletes the renamed app-data directories on each test install per the locked-in decision.

P5 lands last; it is structurally incompatible with P1–P4 being mid-flight, because the directory move invalidates every relative path the prior phases edited.

## Predecessors

P1 (UI strings), P2 (docs + ADR), P3 (internal Rust/TS symbols, including file renames such as `plugin_install.rs` → `gadget_install.rs` and `plugin_host.rs` → `gadget_host.rs`), and P4 (public/wire surface: WIT contents, manifest section name, Tauri command names, settings key prefix, bundled.toml key, WIT filename `torchsnap-plugin.wit` → `torchsnap-gadget.wit` *inside* `plugins/plugin-sdk/wit/`) must all be merged before P5 starts.

## Successors

After P5 merges, execute **P2b** — the second tranche of documentation rewrites that was deferred because it quotes paths and symbols that only exist post-P5. The full list of P2b commits is in `01kqwvtgqpvxb64eysw7pt4gdt-phase-2-docs-and-adr.md` under "Commit grouping", commits 9–14:

- `update README and CLAUDE.md gadget terminology`
- `rewrite docs/Gadget-Architecture content for gadget terminology` (includes `05-plugin-messaging.md` → `05-gadget-messaging.md`)
- `rename and rewrite docs/api/plugin-development.md to gadget-development`
- `update docs/api/logging-system.md gadget terminology`
- `rewrite docs/strategy/Selfcontained-Gadget-System content`
- `update docs/Howto-build-on-fedora-43.md gadget terminology`

Once P2b is done, close the trigger todo `todos/01kqw4kqe3g50xn77efwv71jy2-rename-plugins-to-gadgets-everywhere.md`.

## Scope

### In scope (P5 owns these)

- **Workspace dir moves (via `git mv`):** `plugins/` → `gadgets/`, `plugins/plugin-sdk/` → `gadgets/gadget-sdk/`, `packages/plugin-sdk/` → `packages/gadget-sdk/`, `src/plugins/` → `src/gadgets/`.
- **Test-fixture dir moves (via `git mv`):** `src-tauri/tests/fixtures/{assets,command,failing-enable,minimal,opener-http,website-metadata}-plugin/` → `*-gadget/`, plus the committed `*_plugin.wasm` artifact inside each → `*_gadget.wasm`.
- **Per-plugin Cargo crate rename:** `bangs-plugin` → `bangs-gadget` (and analogous for `calculator`, `emoji-picker`, `hello-world`, `open-url`, `template`, `zerotier`); `[lib]` name fields where present (none currently set explicitly — they default from `[package].name`); test-fixture crate names `torchsnap-test-*-plugin` → `torchsnap-test-*-gadget`. The downstream `.wasm` artifact filename change (`bangs_plugin.wasm` → `bangs_gadget.wasm`) is coupled to this rename and stays in P5.
- **Per-plugin manifest `wasm = "..."` filename** (the *value*, not the section header which P4 renamed): updated to track the new artifact name.
- **Just file rename and recipe renames:** `just/plugins.just` → `just/gadgets.just`; every recipe inside it; every cross-reference in other `.just` files; `Justfile` `import` line.
- **Bundle staging dir rename:** `target/bundled-plugins/` → `target/bundled-gadgets/`. Updates: `tauri.conf.json` `bundle.resources` map, `.gitignore` comments, `just/maintenance.just` clean target, `just/plugins.just` `BUNDLED_DIR` constant.
- **App-data path string literals (no migration code):** every `app_data_dir.join("plugins")` and `app_data_dir.join("plugin-home")` plus surrounding rustdoc and inline test-TOML strings.
- **Host-side `wit_bindgen::generate!` path:** `src-tauri/src/wasm/bindings.rs:23` (`path: "../plugins/plugin-sdk/wit"` → `"../gadgets/gadget-sdk/wit"`). The `world` value on line 24 was already changed in P4.
- **Host-side dev-discovery path:** `src-tauri/src/wasm/discovery.rs:74` (`Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugins")` → `"../gadgets"`).
- **Host-side resource-discovery string:** `<resource_dir>/plugins/` (Rust string literal `"plugins"`) → `"gadgets"` everywhere it references the bundled tree.
- **Cargo workspace files:** `gadgets/Cargo.toml` `members = […]` updated for renamed member dirs; `gadgets/.cargo/config.toml` (a directory move only — content unchanged).
- **npm workspace plumbing:** every per-plugin `frontend/package.json` `"@torchsnap/plugin-sdk": "file:../../../packages/plugin-sdk"` → `"@torchsnap/gadget-sdk": "file:../../../packages/gadget-sdk"`. (The dependency *name* `@torchsnap/gadget-sdk` is P4's; the dep *path* `packages/gadget-sdk` is P5's; both edits land together because they sit on the same line.)
- **Lock files:** regenerate `gadgets/Cargo.lock` (the moved `plugins/Cargo.lock`) and `bun.lock` after the path/name edits.
- **Tools:** `tools/list-bundled-plugins` → `tools/list-bundled-gadgets` (file rename + tomlPath default + docstrings).
- **Devcontainer:** `.devcontainer/devcontainer.json` `rust-analyzer.linkedProjects` entry; `.devcontainer/Dockerfile` comments.
- **`.gitignore`:** comments at lines 27–31, paths at 32, 38, 39, 41, 43, 57.
- **vite.config.ts files:** root (`/src/plugins/` → `/src/gadgets/` in the `manualChunks` rule, line 48) and per-plugin — only path-shape references.
- **Source-file rustdoc/JSDoc** that mention the now-renamed paths.

### Out of scope (other phases)

- WIT package, world, file *content* — P4.
- WIT *file* rename (`torchsnap-plugin.wit` → `torchsnap-gadget.wit`) inside the still-original parent dir — P4 does the rename, P5 then moves the parent dir.
- Tauri command names, Tauri event names (`open-plugin-settings` → `open-gadget-settings`, `activate-plugin-custom-ui` → `activate-gadget-custom-ui`), event payload keys, settings key prefix, source-discriminator wire string, manifest `[plugin]` section header, `bundled.toml` `plugins = [...]` key, permission-var tokens (`${plugin-data}` → `${gadget-data}`, `${plugin-archive}` → `${gadget-archive}`) — P4.
- The legacy migration block at `plugin_host.rs:207–215` is deleted by P4 (no-migration decision) — P5 does not reference or remove it.
- All internal symbol renames (`PluginHost`, `PluginSlot`, etc.) and the `plugin_install.rs` / `plugin_host.rs` etc. file renames — P3.
- UI strings — P1. Docs / ADRs — P2.
- External Tauri ecosystem packages (`@tauri-apps/plugin-*`, `tauri-plugin-store`, `tauri-plugin-opener`, …) — never renamed.
- ESLint plugin packages (`eslint-plugin-react-hooks`, …) — external, never renamed.
- Vite's own `Plugin` type import — Vite-owned, never renamed.

## Scope-split rules

The following rules govern what belongs to P4 versus P5 at every cross-phase boundary:

1. **WIT file rename split.** P4 owns `torchsnap-plugin.wit` → `torchsnap-gadget.wit` (file content change + filename); P5 owns the parent-directory move. The host-side `wit_bindgen::generate!` `path:` literal in `src-tauri/src/wasm/bindings.rs:23` is updated by P5 (path-only).
2. **Per-plugin crate rename phase.** Stays in P5. The crate-name change cascades to the Cargo-emitted `.wasm` filename and to each manifest's `wasm = "..."` value; both are intrinsically tied to artifact paths (P5's domain).
3. **`tools/list-bundled-plugins` rename.** Rename and update in P5; the script's reading of the new TOML key (`gadgets = [...]`) is a P4 dependency P5 lands after.
4. **Per-plugin `wasm = "..."` value vs. `[plugin]` section header.** P4 changes the section header `[plugin]` → `[gadget]`; P5 changes the `wasm = "..."` value inside it. Same files, line-level partition; P5 lands after P4 so the conflict resolves itself.
5. **`bundled.toml` contents.** The plugin-id list (`"calculator"`, `"emoji-picker"`, `"bangs"`, `"open-url"`, `"zerotier"`) is unchanged — those are gadget IDs, not crate names. The TOML *key* (`plugins = […]`) is P4. P5 only moves the file along with the directory.
6. **App-data migration code.** Explicitly NO migration code. Manual cleanup steps documented at the end of this plan.
7. **`tauri.conf.json` `bundle.resources` map.** Both the source and destination path endpoints rename atomically in P5: `"../target/bundled-plugins/*.torchsnap": "plugins/"` → `"../target/bundled-gadgets/*.torchsnap": "gadgets/"`. The host-side `"gadgets"` string literal in `discovery.rs` must change in the same commit or release builds load nothing.

## Workspace directory rename plan

### `plugins/` → `gadgets/`

`git mv plugins gadgets` is the one move that triggers most of the cascade. Before invoking it, every consumer of the old path must be queued for the same commit. Concretely:

**Cargo workspace consumers** (relative paths from the repo root):

- `gadgets/Cargo.toml` (the moved `plugins/Cargo.toml`): `members = […]` entries — only the `"plugin-sdk"` entry needs an inner-dir update to `"gadget-sdk"`; `"hello-world"`, `"calculator"`, `"bangs"`, `"emoji-picker"`, `"open-url"`, `"template"`, `"zerotier"` are unchanged. Comment block references to "plugin workspace" rewritten.
- `just/install.just:23` `cargo fetch --manifest-path plugins/Cargo.toml` → `gadgets/Cargo.toml`.
- `just/maintenance.just:10–12`: `cargo clean --manifest-path plugins/plugin-sdk/Cargo.toml` → `gadgets/gadget-sdk/Cargo.toml`; `rm -rf plugins/target` → `gadgets/target`.
- `just/quality.just:21,29` comments and the `cd plugins && cargo test …` invocation in `test-plugins`.
- `just/plugins.just` (becomes `just/gadgets.just`): `WASM_BUILD_DIR := "plugins/target/wasm32-wasip2/release"` → `gadgets/target/wasm32-wasip2/release`; every `plugins/{{ name }}` → `gadgets/{{ name }}`; every `plugins/*/` glob; every `plugins/{{ name }}.torchsnap`; `tools/list-bundled-plugins plugins/bundled.toml` → `tools/list-bundled-gadgets gadgets/bundled.toml`; the loop variable `dir in plugins/*/` → `dir in gadgets/*/`; the `if [ ! -d "plugins/$id" ]` check; `cd plugins && cargo build …`; `cp "plugins/$id.torchsnap" …`; the WIT recipes' `plugins/plugin-sdk/wit/` → `gadgets/gadget-sdk/wit/`.
- `just/build.just`: every `plugins/` reference in comments at 8–12 plus `just stage-bundled-plugins` (recipe-name change, not a path) and `just build-plugins` (recipe-name change) at 17–18.
- `just/bangs.just:12,16`: comment `plugins/bangs/assets/` and `dst="plugins/bangs/assets/bang.json"` → `gadgets/bangs/assets/bang.json`.

**Rust host consumers** (require the `git mv` to land atomically with the same commit):

- `src-tauri/src/wasm/bindings.rs:23` `path: "../plugins/plugin-sdk/wit"` → `path: "../gadgets/gadget-sdk/wit"`.
- `src-tauri/src/wasm/discovery.rs:74` `Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugins")` → `.join("../gadgets")`. Surrounding doc-comments at lines 14–25, 48, 268.
- `src-tauri/src/wasm/discovery.rs` string literal `"plugins"` at lines 61, 84, 283, 300, 319, 321 (host expects bundled tree under `<resource_dir>/plugins/` and user tree under `<app_data_dir>/plugins/`) → `"gadgets"`.
- `src-tauri/src/wasm/source.rs:58,62` rustdoc references.
- `src-tauri/src/lib.rs:765,772,774,776` rustdoc paths.
- `src-tauri/src/gadgets/clipboard/mod.rs:291–292` (P3-renamed parent dir from `src-tauri/src/plugins/`) doc-comment references to `plugins/`.
- `src-tauri/src/wasm/bridge.rs:176` rustdoc.

**Tauri config consumer:**

- `src-tauri/tauri.conf.json` `bundle.resources`: `"../target/bundled-plugins/*.torchsnap": "plugins/"` → `"../target/bundled-gadgets/*.torchsnap": "gadgets/"`. Both endpoints change in lockstep with the host-side `"gadgets"` string literals above.

**Devcontainer consumer:**

- `.devcontainer/devcontainer.json:58` `"plugins/Cargo.toml"` → `"gadgets/Cargo.toml"` in `rust-analyzer.linkedProjects`.
- `.devcontainer/Dockerfile:167,180` comment references.

**`.gitignore` consumer:**

- Lines 27–31: comment block referencing `target/bundled-plugins/`.
- Line 32: `/target/` (unchanged path, but the *comment* explaining it changes).
- Lines 38–43: `plugins/target/`, `plugins/*/target/`, `plugins/*/*.wasm`, `plugins/*.torchsnap` → `gadgets/…`.
- Line 57: `plugins/bangs/assets/bang.json` → `gadgets/bangs/assets/bang.json`.

**TS/frontend consumers** (path strings, not symbol names):

- `vite.config.ts` (root, line 48) `!id.includes("/src/plugins/")` — see the `src/plugins/` → `src/gadgets/` subsection below; that move is P5's and this line changes as part of it.

### `plugins/plugin-sdk/` → `gadgets/gadget-sdk/`

Inside the larger `plugins/` → `gadgets/` move. Specific path consumers requiring updates beyond the parent move:

- `gadgets/Cargo.toml` `members = ["plugin-sdk", …]` → `members = ["gadget-sdk", …]` (one entry).
- Each plugin's `Cargo.toml` `[dependencies] torchsnap-plugin-sdk = { path = "../plugin-sdk" }` → `torchsnap-gadget-sdk = { path = "../gadget-sdk" }`. (The crate *name* on the LHS was renamed by P4; this update tracks the path RHS.) Files: `gadgets/{bangs,calculator,emoji-picker,hello-world,open-url,template,zerotier}/Cargo.toml` (7 plugins).
- `just/maintenance.just:10` `--manifest-path plugins/plugin-sdk/Cargo.toml` → `gadgets/gadget-sdk/Cargo.toml`.
- `just/plugins.just` (now `just/gadgets.just`) `wit/` recipe paths: `plugins/plugin-sdk/wit/` → `gadgets/gadget-sdk/wit/` (lines 271, 277, 285).
- `src-tauri/src/wasm/bindings.rs:23` already covered above.

### `packages/plugin-sdk/` → `packages/gadget-sdk/`

- Each plugin frontend `package.json` (5 files: `bangs`, `calculator`, `emoji-picker`, `template`, `zerotier`): `"@torchsnap/plugin-sdk": "file:../../../packages/plugin-sdk"` → `"@torchsnap/gadget-sdk": "file:../../../packages/gadget-sdk"`. Both halves of the line change in lockstep (LHS is P4's, RHS is P5's, but they edit the same line).
- `src/contexts/GadgetContext.tsx:94` (P3-renamed from `PluginContext.tsx`) rustdoc-style comment referencing `packages/plugin-sdk/src/shims/hooks.ts` — update path to `packages/gadget-sdk/src/shims/hooks.ts`.
- `src/contexts/GadgetContext.tsx:109` actual deep import `"../../packages/plugin-sdk/src/shims/hooks"` → `"../../packages/gadget-sdk/src/shims/hooks"`.
- `src/lib/sdk.ts:14,56` rustdoc-style comments.

### `src/plugins/` → `src/gadgets/`

`git mv src/plugins src/gadgets`. The directory contains `clipboard/`, `registry.ts`, `types.ts`, and `wasmPluginLoader.ts`. Symbols inside these files were already renamed by P3; P5 only moves the directory and updates the path consumers.

**Path consumers requiring updates:**

- `vite.config.ts` (root, line 48): `!id.includes("/src/plugins/")` → `!id.includes("/src/gadgets/")` in the `manualChunks` rule.
- Import statements in `src/` files that reference the `src/plugins/` directory by relative path — update each to the renamed directory. Concrete sites (pre-P3 line numbers; P3 only renamed symbols, not import path strings, so the offsets carry over): `src/launcher/main.tsx:13` and `src/settings/main.tsx:13` `"../plugins/wasmPluginLoader"`; `src/launcher/Launcher.tsx:21,22` `"../plugins/registry"` and `"../plugins/types"`; `src/settings/SettingsPanel.tsx:6,7` `"../plugins/registry"` and `"../plugins/types"`; `src/settings/sections/GadgetsManagementPanel.tsx:27` (P3-renamed from `PluginsManagementPanel.tsx`) `"../../plugins/registry"`. P5 updates the path segment `plugins/` → `gadgets/` only.
- `src-tauri/src/gadgets/clipboard/mod.rs` (P3-renamed parent dir) path comments that reference `src/plugins/` — update to `src/gadgets/`.

## Per-plugin crate rename plan

Confirmed by reading each `Cargo.toml`. None of the per-plugin crates set an explicit `[lib] name = …`, so the lib name defaults from `[package].name` and the cdylib output filename is automatic.

Per crate (file = `gadgets/<id>/Cargo.toml`):

| Plugin id | Old `[package].name` | New `[package].name` | Old `.wasm` artifact | New `.wasm` artifact |
| --- | --- | --- | --- | --- |
| bangs | `bangs-plugin` | `bangs-gadget` | `bangs_plugin.wasm` | `bangs_gadget.wasm` |
| calculator | `calculator-plugin` | `calculator-gadget` | `calculator_plugin.wasm` | `calculator_gadget.wasm` |
| emoji-picker | `emoji-picker-plugin` | `emoji-picker-gadget` | `emoji_picker_plugin.wasm` | `emoji_picker_gadget.wasm` |
| hello-world | `hello-world-plugin` | `hello-world-gadget` | `hello_world_plugin.wasm` | `hello_world_gadget.wasm` |
| open-url | `open-url-plugin` | `open-url-gadget` | `open_url_plugin.wasm` | `open_url_gadget.wasm` |
| template | `template-plugin` | `template-gadget` | `template_plugin.wasm` | `template_gadget.wasm` |
| zerotier | `zerotier-plugin` | `zerotier-gadget` | `zerotier_plugin.wasm` | `zerotier_gadget.wasm` |

Per crate, the dependency `torchsnap-plugin-sdk = { path = "../plugin-sdk" }` becomes `torchsnap-gadget-sdk = { path = "../gadget-sdk" }`.

Per crate's `manifest.toml` (note: P4 already renamed the `[plugin]` section header to `[gadget]`), update only the `wasm = "<filename>"` value to match the new artifact name. Comments inside each `manifest.toml` referring to "plugin" terminology are P1/P2/P3 territory by line, but path-related comments (e.g. `# the .wasm output of cargo build`) are P5.

The SDK crate `gadgets/gadget-sdk/Cargo.toml`: P4 already renamed `[package].name = "torchsnap-plugin-sdk"` → `"torchsnap-gadget-sdk"`. P5 only does the directory move.

`just/plugins.just` no longer has any line that hardcodes `*-plugin` crate names — it derives the crate name from `Cargo.toml` via `tools/toml-get`, so the recipe stays compatible after the rename without further edits.

## Test fixture rename plan

Six fixture directories under `src-tauri/tests/fixtures/`:

- `assets-plugin/` → `assets-gadget/` (artifact `assets_plugin.wasm` → `assets_gadget.wasm`; crate name `torchsnap-test-assets-plugin` → `torchsnap-test-assets-gadget`)
- `command-plugin/` → `command-gadget/` (`command_plugin.wasm` → `command_gadget.wasm`; `torchsnap-test-command-plugin` → `torchsnap-test-command-gadget`)
- `failing-enable-plugin/` → `failing-enable-gadget/` (`failing_enable_plugin.wasm` → `failing_enable_gadget.wasm`; `torchsnap-test-failing-enable-plugin` → `torchsnap-test-failing-enable-gadget`)
- `minimal-plugin/` → `minimal-gadget/` (`minimal_plugin.wasm` → `minimal_gadget.wasm`; `torchsnap-test-minimal-plugin` → `torchsnap-test-minimal-gadget`)
- `opener-http-plugin/` → `opener-http-gadget/` (`opener_http_plugin.wasm` → `opener_http_gadget.wasm`; `torchsnap-test-opener-http-plugin` → `torchsnap-test-opener-http-gadget`)
- `website-metadata-plugin/` → `website-metadata-gadget/` (`website_metadata_plugin.wasm` → `website_metadata_gadget.wasm`; `torchsnap-test-website-metadata-plugin` → `torchsnap-test-website-metadata-gadget`)

Per fixture, six edits:

1. `git mv` the directory.
2. `git mv` the committed `<name>_plugin.wasm` artifact to `<name>_gadget.wasm` inside it. (These artifacts are tracked in git per the comment in `.gitignore` lines 45–48.)
3. Update fixture `Cargo.toml` `[package].name` to the `*-gadget` form.
4. Update fixture `manifest.toml` `wasm = "<name>_plugin.wasm"` to `<name>_gadget.wasm`. (Section header `[plugin]` → `[gadget]` is P4.)
5. Update Rust test code that constructs paths to the fixture (see next section).
6. Update fixture rustdoc and `src/lib.rs` comments referencing the old filename. (`tests/fixtures/minimal-plugin/src/lib.rs:14` says "produce `minimal_plugin.wasm`".)

Test-code consumers requiring path-string updates:

- `src-tauri/src/wasm/runtime/mod.rs:84` doc-comment.
- `src-tauri/src/wasm/runtime/mod.rs:88` `include_bytes!("../../../tests/fixtures/minimal-plugin/minimal_plugin.wasm")` → `…/minimal-gadget/minimal_gadget.wasm`.
- `src-tauri/src/wasm/runtime/mod.rs:208` `"…/website-metadata-plugin/website_metadata_plugin.wasm"` → `gadget` form.
- `src-tauri/src/wasm/runtime/mod.rs:215` `…/opener-http-plugin/opener_http_plugin.wasm` → gadget form.
- `src-tauri/src/wasm/runtime/mod.rs:221` `…/assets-plugin/assets_plugin.wasm` → gadget form.
- `src-tauri/src/wasm/runtime/mod.rs:228` `…/command-plugin/command_plugin.wasm` → gadget form.
- `src-tauri/src/wasm/runtime/mod.rs:1150,1187` `tests/fixtures/assets-plugin` and `tests/fixtures/assets-plugin/data/payload.bin` → gadget form.
- `src-tauri/src/wasm/bridge.rs:1065` `concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures")` — unchanged (the FIXTURE_ROOT itself doesn't carry the `*-plugin` segment).
- `src-tauri/src/wasm/bridge.rs:1151,1152,1162,1288,1289,1304,1365,1366,1382,1489,1490,1500` — every test that joins `"minimal-plugin/minimal_plugin.wasm"` or builds an inline TOML string `wasm = "minimal_plugin.wasm"` for a synthetic plugin manifest. Each needs both the directory segment and the wasm filename updated.
- `src-tauri/src/wasm/bridge.rs:1411` test fn `sql_config_uses_plugin_home_layout` — the test name itself is P3's territory (internal test name), but the inline string contents like `"plugin-home"` paths are P5's.

## Just recipe rename plan

### File rename

`just/plugins.just` → `just/gadgets.just` (`git mv`). Update `Justfile` line 10 `import 'just/plugins.just'` → `import 'just/gadgets.just'`.

### Recipe renames within the file

| Old recipe | New recipe |
| --- | --- |
| `build-plugins` | `build-gadgets` |
| `build-plugin` | `build-gadget` |
| `package-plugin` | `package-gadget` |
| `stage-bundled-plugins` | `stage-bundled-gadgets` |
| `check-plugins` | `check-gadgets` |
| `check-plugin` | `check-gadget` |
| `build-test-fixtures` | unchanged (no `plugin` token in name) |
| `check-wit` | unchanged |
| `fmt-wit` | unchanged |
| `fmt-check-wit` | unchanged |

Internal cross-references inside the file:

- `build-plugins` body calls `just build-plugin "$name"` → `just build-gadget "$name"`.
- `build-plugin` body calls `just package-plugin "{{ name }}"` → `just package-gadget`.
- `package-plugin` error message `Run 'just build-plugin {{ name }}' first` → `build-gadget`.
- `stage-bundled-plugins` body calls `just build-plugin "$id"` → `just build-gadget`.
- Block-comment references to recipe names throughout (e.g. `Stage whitelisted plugins …`, `build-plugin is idempotent`).
- `BUNDLED_DIR := "target/bundled-plugins"` → `"target/bundled-gadgets"`.

### Cross-references in other `.just` files

- `Justfile:10` `import 'just/plugins.just'` → `import 'just/gadgets.just'`.
- `just/build.just:8,9,10,11,17,18` — comments + `just stage-bundled-plugins` + `just build-plugins`.
- `just/quality.just:32` `check: check-crates check-wit check-plugins check-types` → `check-gadgets`. Plus comment at line 21.
- `just/quality.just:10` `test: test-crates test-plugins` — recipe `test-plugins` is defined in `quality.just` and depends on `cd plugins && cargo test …`. The recipe name itself is not in `plugins.just`; rename it to `test-gadgets` here for consistency. Update the body's `cd plugins` → `cd gadgets`.
- `just/quality.just` `test-plugins` recipe at line 28-29 → `test-gadgets`.
- `just/maintenance.just:10–12` already covered.
- `just/bangs.just:10,12,16` already covered.
- `just/install.just:23` already covered.

### Files unaffected

- `just/start.just` — no plugin references.
- `just/doctor.just` — no plugin path refs (mentions WASM toolchain generically).
- `just/tools.just` — no plugin refs.
- `just/devcontainer.just` — no plugin refs.
- `just/assets.just` — no plugin refs.

## Build artifact path rename plan

Atomic-commit unit:

- `src-tauri/tauri.conf.json` `bundle.resources` map: `"../target/bundled-plugins/*.torchsnap": "plugins/"` → `"../target/bundled-gadgets/*.torchsnap": "gadgets/"`.
- `just/plugins.just` (now `gadgets.just`) `BUNDLED_DIR := "target/bundled-plugins"` → `"target/bundled-gadgets"`.
- `just/maintenance.just:12` `rm -rf target/bundled-plugins` → `target/bundled-gadgets`.
- `.gitignore` lines 28–29 comments referring to `target/bundled-plugins/`.
- `src-tauri/src/wasm/discovery.rs:15` doc-comment.

The `<resource_dir>/plugins/` runtime path the host scans: see the host-side string-literal `"plugins"` → `"gadgets"` work in the discovery section above. Updating tauri.conf.json's destination key alone is not enough; the host's scan target must change in the same commit or release builds load nothing.

## App-data storage path rename plan

No migration code per inventory decision. Only string literals and surrounding rustdoc change.

P3 has merged before P5 starts (see Predecessors), so `plugin_install.rs` is `gadget_install.rs` and `plugin_host.rs` is `gadget_host.rs`. The pre-rename line numbers cited below are the offsets P5 walks against the post-P3 files.

### `<app_data>/plugins/` → `<app_data>/gadgets/`

- `src-tauri/src/gadget_install.rs` (P3-renamed from `plugin_install.rs`; lines under the pre-P3 numbering): 25, 34, 35, 143–148 (plus surrounding comments), 150, 164, 219, 221, 227, 230. Each `app_data_dir.join("plugins")` → `.join("gadgets")`. Each rustdoc reference to `<app_data_dir>/plugins/<id>.torchsnap`, `<app_data_dir>/plugins/<id>/` updated.
- `src-tauri/src/wasm/discovery.rs:84,300,319,321` (and corresponding doc-comments at 23, 268).
- `src-tauri/src/wasm/source.rs:62` (doc-comment for the `User` variant).
- `src-tauri/src/wasm/source.rs:58` (doc-comment for the `System` variant — this is `<resource_dir>/plugins/`, not app_data; same rename applies).
- `src-tauri/src/gadget_host.rs:198,210,241,411,845` settings key prefix `plugins.<id>.` is P4's territory (wire surface), not P5's. P5 leaves these alone.

### `<app_data>/plugin-home/<id>/` → `<app_data>/gadget-home/<id>/`

- `src-tauri/src/gadget_install.rs:37,182,235,238,240` plus the rustdoc context.
- `src-tauri/src/gadgets/clipboard/mod.rs:291,292,299` (P3-renamed parent dir from `src-tauri/src/plugins/`).
- `src-tauri/src/wasm/bridge.rs:114,171,173,176,223,276,277,1405,1409,1411,1421,1466,1473,1475,1476`. Each `app_data_dir.join("plugin-home")` → `.join("gadget-home")` and each rustdoc/comment `plugin-home/` → `gadget-home/`.

### Permission var token

- `src-tauri/src/wasm/permission_vars.rs:46` literal `"plugin-data"` → `"gadget-data"` (and `:47` `"plugin-archive"` → `"gadget-archive"`) are **P4's territory** (manifest permission-contract tokens, author-facing wire surface) per the inventory at lines 68–69 — not P5's. P5 leaves them alone.

### Doc-comment updates within each touched file

Every rustdoc paragraph that says "the plugin's home directory at `<app_data_dir>/plugin-home/<id>/`" updated to "gadget-home" for verbal consistency. Located mainly in `bridge.rs` (the SQL storage section) and `gadget_install.rs`. Detailed line list above; P5 scans surrounding 5–10 lines for stale references.

## Cargo + npm workspace plan

### Cargo

- `gadgets/Cargo.toml` (moved): edit `members = […]`, swapping only the `"plugin-sdk"` entry to `"gadget-sdk"`. Other entries are gadget-id-shaped and unchanged.
- Each per-gadget `Cargo.toml`: `[dependencies] torchsnap-plugin-sdk = { path = "../plugin-sdk" }` → `torchsnap-gadget-sdk = { path = "../gadget-sdk" }` (LHS by P4, RHS by P5; one-line edit lands together).
- Each test-fixture `Cargo.toml`: `[package].name` rename only; no inter-crate `path =` deps.
- `gadgets/Cargo.lock` (moved): regenerated by running `cargo metadata --manifest-path gadgets/Cargo.toml` once after the edits.
- `src-tauri/Cargo.toml`: confirmed no path dep on the plugin SDK exists; no edits needed.

### npm

The repo does NOT declare a `workspaces` field in the root `package.json` (verified). The plugin frontends use direct `file:` deps to `packages/plugin-sdk` instead. So no top-level workspace edit is required. The 5 frontend `package.json` files each have a `"@torchsnap/plugin-sdk": "file:../../../packages/plugin-sdk"` line that becomes `"@torchsnap/gadget-sdk": "file:../../../packages/gadget-sdk"`.

`bun.lock` regeneration: run `bun install` after the package.json edits and after the `git mv packages/plugin-sdk packages/gadget-sdk` move. This rewrites every `file:…/packages/plugin-sdk` reference in the lockfile.

### Lock files in commit grouping

Commit the Cargo.lock and bun.lock regeneration in the same commit as the path edits that caused them — they are non-source, no MPL header needed, and will fail CI if regenerated late.

## CI / devcontainer plan

### `.github/workflows/`

Verified absent (`ls .github` shows only the directory, no `workflows/` subdir). No CI files exist in this repo. No edits needed.

### `.devcontainer/`

- `devcontainer.json:58` `"plugins/Cargo.toml"` → `"gadgets/Cargo.toml"` in `rust-analyzer.linkedProjects`.
- `Dockerfile:167` comment `wasm32-wasip2: the target every plugin in plugins/ compiles to.` → `…every gadget in gadgets/…`.
- `Dockerfile:180` comment `WIT files that define the plugin ABI.` → `…gadget ABI.`.
- `NOTICE.md` in `.devcontainer/` — read but only if it mentions plugins; if it does, those references are P2 (docs). P5 skips.

## Commit grouping

P5 should land as a small number of large, semantically grouped commits — each one leaves the repo buildable. Recommended sequence (each line = one commit):

1. **Per-plugin Cargo crate rename** (depends on P4's WIT/SDK rename merging first). Touches: every `plugins/<id>/Cargo.toml` `[package].name` (the directory is still `plugins/` until commit 4), the `wasm = "..."` value in each `plugins/<id>/manifest.toml`, the renamed `*_gadget.wasm` committed artifact next to each plugin root, the `plugins/Cargo.lock` regeneration. Self-contained: with the P4 SDK in place, this just changes crate names and downstream artifact filenames.
2. **Test fixture rename**. Six `git mv` ops on directories, six `git mv` ops on `*.wasm` artifacts, `Cargo.toml` and `manifest.toml` updates per fixture, all `runtime/mod.rs` and `bridge.rs` test-string updates.
3. **Move `packages/plugin-sdk/` → `packages/gadget-sdk/`** and update the 5 frontend `package.json` `file:…` deps + the 2 `src/` deep imports + `bun.lock` regen.
4. **Move `plugins/` → `gadgets/` (and inner `plugin-sdk/` → `gadget-sdk/`) and `src/plugins/` → `src/gadgets/`**. The largest commit. Touches: `git mv` of both directory trees; the moved `gadgets/Cargo.toml` `members` edit (only the `"plugin-sdk"` entry → `"gadget-sdk"`); every per-plugin `Cargo.toml` `torchsnap-gadget-sdk = { path = "../plugin-sdk" }` RHS → `"../gadget-sdk"`; every `plugins/` path-string consumer in the Just files (`just/plugins.just`'s `WASM_BUILD_DIR`, every `plugins/{{ name }}` and `plugins/*/` glob, the `cd plugins` invocations, the WIT recipe paths at lines 271/277/285); `tauri.conf.json`'s `bundle.resources` *destination* key `"plugins/"` → `"gadgets/"` (the source-side `target/bundled-plugins` stays here and renames in commit 5); the host-side `"plugins"` string literals in `src-tauri/src/wasm/discovery.rs` at lines 61, 84, 283, 300, 319, 321 (these must flip in the same commit as the tauri-config destination key — release builds load from `<resource_dir>/<destination>/`); `src-tauri/src/wasm/bindings.rs:23`; `src-tauri/src/wasm/discovery.rs:74` dev-discovery path; `.gitignore` lines 38–43 + 57; `.devcontainer/devcontainer.json:58` and `Dockerfile:167,180`; the root `vite.config.ts:48`; `just/bangs.just:12,16` data path; `just/install.just:23`; `just/maintenance.just:10–11`; `just/quality.just:21,29` `cd plugins`; rename `tools/list-bundled-plugins` → `tools/list-bundled-gadgets` (file rename + tomlPath default + docstring) and update its sole call site at `just/plugins.just:183` to `tools/list-bundled-gadgets gadgets/bundled.toml`. The moved `gadgets/Cargo.lock` is regenerated.
5. **Rename `target/bundled-plugins/` → `target/bundled-gadgets/`** and the staging-recipe rename. This is a thin commit but logically distinct: `tauri.conf.json` `bundle.resources` *source-side* path, `BUNDLED_DIR` constant in `just/plugins.just`, `.gitignore` lines 28–29 comments, `just/maintenance.just:12` clean target. Pair with the recipe-name rename `stage-bundled-plugins` → `stage-bundled-gadgets` (definition in `just/plugins.just` plus call site `just stage-bundled-plugins` in `just/build.just:17`).
6. **Just file rename and remaining recipe renames**. `git mv just/plugins.just just/gadgets.just`; remaining recipe-name renames inside (`build-plugins`, `build-plugin`, `package-plugin`, `check-plugins`, `check-plugin` → gadget equivalents; `stage-bundled-plugins` was already renamed in commit 5); cross-references in other `.just` files (`just/build.just:18` `just build-plugins`, `just/quality.just:32` `check-plugins`, the `test-plugins` recipe in `quality.just:10,28-29` → `test-gadgets`); root `Justfile:10` `import 'just/plugins.just'` → `'just/gadgets.just'`.
7. **App-data path string literals** (no migration code). Touches `gadget_install.rs`, `wasm/bridge.rs`, `wasm/discovery.rs`, `wasm/source.rs`, `gadgets/clipboard/mod.rs`. Plus surrounding rustdoc.
8. **Rustdoc and comment cleanup sweep**. Final pass: every remaining `plugin` / `Plugin` reference in source-file comments that the prior commits missed. Drives the verification sweep at the end.

Each commit should pass `just check` and `just test` standalone.

## Test impact

### High-risk: fixture-loading tests

`src-tauri/src/wasm/runtime/mod.rs` and `src-tauri/src/wasm/bridge.rs` contain `include_bytes!` macros and inline `std::fs::copy` operations that hardcode the fixture paths. A miss here breaks the entire host integration test suite — covered by the explicit line list in the test-fixture section above.

### Medium-risk: tests that build inline plugin manifests

`src-tauri/src/wasm/bridge.rs:1162,1304,1382,1500` each build an inline TOML string like `wasm = "minimal_plugin.wasm"` for a synthetic plugin manifest. Each must be updated to `minimal_gadget.wasm` to match the renamed fixture artifact.

### Low-risk: tests that compute paths from constants

`src-tauri/src/wasm/bridge.rs:1065` `FIXTURE_ROOT` constant uses `concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures")` — unchanged because the renamed dir name is appended downstream. No edit.

### TS/frontend tests

The packages/`gadget-sdk` (post-rename) test setup at `packages/gadget-sdk/src/testing/setup.ts` references `@torchsnap/plugin-sdk` (now `@torchsnap/gadget-sdk` post-P4). All path-string references inside `src/testing/` are P3 or P4 territory; P5 only does the file move via `git mv` of the parent directory.

### Sanity tests after rename

After all P5 commits land, run `just test` and `just test-gadgets` (renamed `test-plugins`) end-to-end. Specific test cases to watch:

- `sql_config_uses_plugin_home_layout` (now needs renaming to `…gadget_home…` if P3 didn't handle internal Rust test names). Actually P3 owns test fn names; P5 leaves the name. P5 only updates the *string contents* the test asserts over.
- Discovery tests (`discovery.rs:283–321`) that create `plugins/` subdirs for the test fixture — every `"plugins"` literal becomes `"gadgets"`, including in `tempdir.join("plugins")` inside the test bodies.

## Documentation impact within source files

Every source file P5 touches gets a rustdoc/JSDoc sweep. Notable concentrations:

- `src-tauri/src/wasm/discovery.rs` lines 5–33 (the `Plugin Discovery` block comment): rewrite "Plugin Discovery", "system plugins", "dev plugins", "user plugins" to "Gadget Discovery", "system gadgets", etc.
- `src-tauri/src/wasm/bridge.rs` lines 100–200 (the SQL storage rustdoc) referencing `plugin-home/<plugin-id>/` paths.
- `src-tauri/src/gadget_install.rs` opening rustdoc block.
- `gadgets/Cargo.toml` opening header comment ("Torchsnap WASM Plugin Workspace").
- `gadgets/.cargo/config.toml` opening header comment ("Plugin-workspace cargo defaults").
- Each per-gadget `Cargo.toml`'s 1-line target comment ("This crate targets wasm32-wasip2 …").
- Each per-gadget `manifest.toml` is touched only on the `wasm = "..."` line by P5; comment updates inside (`# Prefix-mode custom UI…`) belong to whichever phase already touched the relevant `[plugin]` section header (P4).
- `tools/list-bundled-gadgets` (renamed) docstring lines 7–18.
- `.devcontainer/Dockerfile:167,180` comments.
- `.gitignore` lines 27–43, 45–48, 57.

P5's final sweep commit (commit 8 above) catches anything missed.

## Verification steps

Run after each commit and at the end of P5. Exact commands:

```
# Path-residue sweep — should produce only Tauri/eslint ecosystem hits
rg -n 'plugin' --type rust --type toml src-tauri gadgets packages tools just Justfile src-tauri/tauri.conf.json
rg -n '"plugins"|"plugin-home"|bundled-plugins|plugin-data'
rg -n 'plugin' .gitignore .devcontainer/

# Cargo correctness
cargo fetch --manifest-path src-tauri/Cargo.toml
cargo fetch --manifest-path gadgets/Cargo.toml
cargo metadata --manifest-path gadgets/Cargo.toml --format-version 1 \
  | jq -r '.packages[].name' | sort   # confirm seven *-gadget + torchsnap-gadget-sdk

# Build the gadget workspace
(cd gadgets && cargo build --release)

# Build test fixtures (only if any fixture sources changed beyond the rename)
just build-test-fixtures

# Stage and bundle gadgets
just stage-bundled-gadgets
just build-gadgets
just build           # end-to-end Tauri build (debug)

# Quality gates
just check           # check-crates check-wit check-gadgets check-types
just test            # test-crates test-gadgets
just lint
just fmt-check

# Frontend
bun install          # rewrites bun.lock
bun ls @torchsnap/gadget-sdk
bun run tsc --noEmit

# WIT
just check-wit
just fmt-check-wit

# Manual launcher run with renamed app-data dirs
# (See the manual-cleanup section below.)
```

The most important sweeps:

```
rg -n 'plugins/' --type-not lock | grep -v node_modules | grep -v 'tauri-plugin-' | grep -v 'eslint-plugin-' | grep -v '@tauri-apps/plugin-'
rg -n 'plugin-home|plugin_home'
rg -n '\.plugins\.|plugins = \['   # P4 territory but P5 should still see zero
```

Every remaining hit should be either Tauri ecosystem (`tauri-plugin-store`, `@tauri-apps/plugin-*`), ESLint ecosystem, Vite's own `Plugin` type, or an MPL/license/devcontainer NOTICE line that legitimately references "plugin" in upstream-quoted text.

## Risks & rollback

### Highest risk: missed path

Forgetting to update one of the half-dozen `include_bytes!` paths in `runtime/mod.rs` would surface as a compile error inside `src-tauri/`, caught by `just check-crates`. Forgetting to update `tauri.conf.json`'s `bundle.resources` would silently produce a release build with no system gadgets — caught only by an end-to-end install-and-launch. Mitigation: the verification sweeps above are part of the per-commit checklist, not just the final check.

### Second risk: half-renamed mid-flight

If commit 4 (the big workspace move) lands but commit 5 (bundle-staging dir rename) does not, the build breaks at `just build` because `tauri.conf.json` references a path that the staging recipe no longer populates. Mitigation: keep commits 4 and 5 in adjacent merges; if the merge is staged across two days, hold the merge of commit 4 until commit 5 is reviewed.

### Third risk: lock-file drift

If `Cargo.lock` and `bun.lock` are regenerated locally but not committed, CI/reviewers see the path move without the dependency-graph update and rebuild fails. Mitigation: every commit that edits a `Cargo.toml` or `package.json` regenerates and includes the corresponding lock file.

### Fourth risk: test-fixture wasm artifact mismatch

The committed `*.wasm` artifacts inside test fixtures must be regenerated when the fixture's crate name changes (because the WASM module's metadata embeds the package name). After commit 1 lands, run `just build-test-fixtures` and commit the regenerated `.wasm` files — the rename of the `.wasm` filename via `git mv` is a file-move only; the *contents* must be rebuilt to track the new crate name. Mitigation: commit 2 of the sequence above explicitly includes `just build-test-fixtures` regeneration.

### Rollback

Each commit is small enough to revert individually. The directory `git mv`s are reversible by `git revert`. The staging-dir rename is one config edit. The biggest revert is commit 4; revert leaves the repo in the pre-P5 state cleanly because commit 4 is self-contained.

## Manual cleanup steps for the user

Per the locked-in no-migration decision, before launching the renamed code on any test install, the user manually deletes the following directories. Add these instructions to the trigger todo's closing notes and to the new ADR's `Manual upgrade` section:

**macOS** (defaults shown; user-specific paths may vary):

```
rm -rf "$HOME/Library/Application Support/app.torchsnap/plugins"
rm -rf "$HOME/Library/Application Support/app.torchsnap/plugin-home"
```

**Linux:**

```
rm -rf "$HOME/.local/share/app.torchsnap/plugins"
rm -rf "$HOME/.local/share/app.torchsnap/plugin-home"
```

**Windows:**

```
Remove-Item -Recurse -Force "$env:APPDATA\app.torchsnap\plugins"
Remove-Item -Recurse -Force "$env:APPDATA\app.torchsnap\plugin-home"
```

Also delete the per-test-install staging artifacts:

```
rm -rf target/bundled-plugins
rm -rf plugins/target
rm -rf plugins/*/target
rm -f plugins/*.torchsnap
```

(These last paths cease to exist post-rename; they cover any *checkout* the user already had pre-rename. After P5 lands, the gitignored equivalents under `gadgets/` and `target/bundled-gadgets/` are recreated automatically by the next `just build`.)

Settings keys persisted by `tauri-plugin-store` under the `plugins.<id>.<key>` prefix were renamed to `gadgets.<id>.<key>` by P4. P4's plan owns the manual cleanup of the settings store JSON file (or, equivalently, P4 documents that the user must reset their settings on first launch); P5 only reminds the user to do so as part of the same "manual upgrade" checklist.

---

### Critical Files for Implementation

- /Users/jakob/Development/github/jakobwesthoff/torchsnap/just/plugins.just
- /Users/jakob/Development/github/jakobwesthoff/torchsnap/src-tauri/tauri.conf.json
- /Users/jakob/Development/github/jakobwesthoff/torchsnap/src-tauri/src/wasm/discovery.rs
- /Users/jakob/Development/github/jakobwesthoff/torchsnap/src-tauri/src/wasm/bindings.rs
- /Users/jakob/Development/github/jakobwesthoff/torchsnap/plugins/Cargo.toml

---

## Confirmation

Phase 5 is fully scoped above as a paths-and-tooling-only phase that lands after P1–P4 have merged, in eight semantically grouped commits that each leave the repo buildable. Every cross-phase boundary that surfaced during exploration has been resolved explicitly: WIT file rename is P4's, parent-directory move is P5's; per-plugin crate rename stays in P5 because it cascades into artifact filenames; `tools/list-bundled-plugins` becomes `tools/list-bundled-gadgets` in P5 after P4 changes the TOML key it reads; `bundled.toml` *contents* (gadget IDs) are unchanged but the file moves with the directory. The biggest residual risk is silent breakage from a missed string literal — mitigated by the explicit `rg` sweeps in verification.

