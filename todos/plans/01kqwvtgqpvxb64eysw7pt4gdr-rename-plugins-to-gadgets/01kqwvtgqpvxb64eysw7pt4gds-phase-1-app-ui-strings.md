# Phase 1 — App UI Strings

## Summary

Phase 1 replaces every user-visible "Plugin"/"plugins" string in the Torchsnap launcher app with "Gadget"/"gadgets". Scope is strictly visible TSX copy: section headings, sidebar nav labels, descriptions, button labels, banner messages, empty states, source-badge tooltips, the OS file-picker filter name, and one developer-readable `throw new Error` whose text uses the *concept* "plugin view". No symbol names, no internal route ids, no Tauri command strings, no source-discriminator wire strings, no comments. The change is a copy-only diff, mechanical, with no behavioural risk.

## Predecessors / dependencies

None. P1 is independent. It does not depend on P2 (docs), P3 (internal symbol rename), P4 (wire surface), or P5 (repo layout). It can land before any of them.

It does carry one consequence for later phases: once these strings flip to "Gadget", screenshots and walkthroughs in `docs/Plugin-Architecture/` (and any product copy pulled into marketing) will be visually inconsistent with the renamed app until P2 lands. This is acceptable per the inventory (pre-alpha, single consumer) and was confirmed by the user.

## Scope

**In:** Visible English copy in TSX rendered to the user — JSX text nodes, `aria-label`, `title=` attributes shown as tooltips, button text, headings, descriptions, OS-dialog filter `name`, banner/toast messages, empty-state messages. One developer-readable error message whose text mentions the concept "plugin view" (not a symbol name) is included on the principle that "concept words = P1, symbol names = P3" — see the discriminator rule below.

**Out:**
- TS/Rust identifiers, type names, props, hook names, file names, CSS class strings, `data-*` attributes — all P3.
- Tauri command name strings (e.g. `command("plugin_sources")`) — P4.
- Source-discriminator wire strings (`item.source.type === "plugin"`) — P4.
- Settings-store key prefix (`plugins.<id>.<key>`) — P4.
- Internal route id `"plugins"` in `SettingsPanel.tsx` — internal, P3 (note the asymmetry vs. the visible `label`).
- Comments and JSDoc — P2 (docs) or P3 (when they sit on a symbol being renamed).
- Error messages that name internal symbols (`<PluginContextProvider>`, `usePluginInfo`, `usePluginRuntime`, `useLauncher`) — those messages move with their symbols in P3.
- Marketing site copy (`web/`) — confirmed: directory does not exist in this repo (`ls /Users/jakob/Development/github/jakobwesthoff/torchsnap` shows no `web/`). Marketing is in a separate repo and out of scope.
- The `clipboard` plugin's frontend copy (`ClipboardSettings.tsx`, `ClipboardView.tsx`) — visible strings there are clipboard-domain (e.g. "Retention", "History"), not the word "plugin". No changes.

## Discriminator rules

Applied consistently throughout this plan; included here so the implementing engineer sees the precedent and can apply it to anything new that turns up:

1. **Concept-word vs. symbol-name in thrown errors.** Decision: error strings using the lowercase concept "plugin"/"plugin view" rename in P1. Error strings naming a symbol like `<PluginContextProvider>` move with that symbol in P3. Applies to `Launcher.tsx:452` (P1) and to the three context hooks (P3 only).
2. **`{ id: "plugins", label: "Plugins" }` (`SettingsPanel.tsx:28`).** Decision: rename `label` to `"Gadgets"` in P1; keep `id: "plugins"` and the `activeSection === "plugins"` check (line 109) as internal routing — handle in P3 alongside other internal identifier renames. Document this asymmetry inline so a follow-up engineer doesn't conflate them.
3. **`name: "Torchsnap Plugin"` in OS file-picker filter (`PluginsManagementPanel.tsx:101`).** Decision: rename to `"Torchsnap Gadget"`. The file extension `.torchsnap` is unchanged (inventory line 87 — product name).
4. **"Built-in" / "System" / "User" / "Dev" badge labels (`PluginsManagementPanel.tsx:280–302`).** These do not contain the word "Plugin", but their tooltips do. Tooltips rename; labels stay.

If the implementing engineer encounters a string this plan does not cover, the rule is: visible English noun "plugin" → "gadget"; identifier or wire token → leave alone for P3/P4.

## File-by-file plan

Line numbers reflect the working-copy state at planning time and may drift by a small amount once edits cascade — match by string, not by line.

### 1. `/Users/jakob/Development/github/jakobwesthoff/torchsnap/src/settings/SettingsSidebar.tsx`

- **Line 57**: `<SidebarGroup label="Plugins" items={pluginItems} ...` → `<SidebarGroup label="Gadgets" items={pluginItems} ...`
  Note: the prop name `pluginItems` and the type field `pluginItems` (line 25, 39) stay — they are identifiers, P3.

### 2. `/Users/jakob/Development/github/jakobwesthoff/torchsnap/src/settings/SettingsPanel.tsx`

- **Line 28**: `{ id: "plugins", label: "Plugins", icon: "heroicons:puzzle-piece" }` → `{ id: "plugins", label: "Gadgets", icon: "heroicons:puzzle-piece" }`
  ONLY `label` changes. `id` stays "plugins" — it is an internal route id used by `activeSection === "plugins"` at line 109 and by tests/keyboard nav if any. Renaming the route id belongs to P3.

### 3. `/Users/jakob/Development/github/jakobwesthoff/torchsnap/src/settings/sections/PluginsManagementPanel.tsx`

This is the densest file. Changes:

- **Line 101**: `filters: [{ name: "Torchsnap Plugin", extensions: ["torchsnap"] }]` → `filters: [{ name: "Torchsnap Gadget", extensions: ["torchsnap"] }]`. Extensions array unchanged.
- **Line 136**: `message: "Only .torchsnap files can be installed as plugins."` → `message: "Only .torchsnap files can be installed as gadgets."`
- **Line 162**: `` message: `Uninstalled plugin "${pluginId}".` `` → `` message: `Uninstalled gadget "${pluginId}".` ``
  (Dynamic-string note: the interpolated `${pluginId}` is the variable name; that variable is renamed in P3. In P1 only the surrounding English changes — `Uninstalled plugin` → `Uninstalled gadget`.)
- **Line 168**: `message: formatError(e, "Failed to uninstall plugin")` → `message: formatError(e, "Failed to uninstall gadget")`
- **Line 177**: `title="Plugins"` (SectionHeader) → `title="Gadgets"`
- **Line 178**: `description="Enable, disable, install, and uninstall plugins. Built-in and system plugins ship with the app and cannot be removed."` → `description="Enable, disable, install, and uninstall gadgets. Built-in and system gadgets ship with the app and cannot be removed."`
- **Line 187**: `<Section title="Installed Plugins">` → `<Section title="Installed Gadgets">`
- **Line 189**: `<div ...>Loading plugins…</div>` → `<div ...>Loading gadgets…</div>`
- **Line 191**: `<div ...>No plugins registered.</div>` → `<div ...>No gadgets registered.</div>`
- **Line 240**: `title="Uninstall this user plugin"` → `title="Uninstall this user gadget"`
- **Line 281**: `tooltip: "Native plugin compiled into the app. Always present.",` → `tooltip: "Native gadget compiled into the app. Always present.",`
- **Line 288**: `tooltip: "WASM plugin bundled with the app. Upgraded when the app is updated; not uninstallable.",` → `tooltip: "WASM gadget bundled with the app. Upgraded when the app is updated; not uninstallable.",`
- **Line 294**: `tooltip: "WASM plugin you installed. Uninstall available."` → `tooltip: "WASM gadget you installed. Uninstall available."`
- **Line 300–302**: `tooltip: "WASM plugin loaded from the repository in debug builds. Release builds never include it.",` → `tooltip: "WASM gadget loaded from the repository in debug builds. Release builds never include it.",`
- **Line 398**: `message: formatError(e, "Failed to install plugin")` → `message: formatError(e, "Failed to install gadget")`

NOT changed in this file (P3/P4 territory):
- The component name `PluginsManagementPanel` (export, file name) — P3.
- Comments at lines 5–20 mentioning "plugin" — P2/P3.
- `command("plugin_sources")` line 65, `command("uninstall_user_plugin", { pluginId })` line 159, `command("install_plugin_archive", ...)` line 389 — P4.
- The `pluginId` parameter name on line 157, 213, 159, 162, 168 — P3.
- The visible `.torchsnap` filename text on the drop-zone (line 329 "Drop a .torchsnap file here, or") — `.torchsnap` is the product extension, unchanged.

### 4. `/Users/jakob/Development/github/jakobwesthoff/torchsnap/src/settings/PluginSettingsWrapper.tsx`

No static visible strings to change. The only visible string is `Enable ${name}` at line 47, where `name` is the dynamic plugin display name passed in by the caller. Plugin display names already come from each gadget's manifest, not the host UI. No P1 change.

(File rename `PluginSettingsWrapper.tsx` → `GadgetSettingsWrapper.tsx` and the prop name `pluginId` are P3.)

### 5. `/Users/jakob/Development/github/jakobwesthoff/torchsnap/src/settings/sections/WebsiteMetadataSection.tsx`

- **Line 79**: `description="Caches website favicons and metadata (title, description) so plugins can show enriched results without repeated network requests."` → `description="Caches website favicons and metadata (title, description) so gadgets can show enriched results without repeated network requests."`

### 6. `/Users/jakob/Development/github/jakobwesthoff/torchsnap/src/devtools/console/LogList.tsx`

- **Line 110**: `<p className="text-xs text-text-muted/70">Log output from plugins will appear here</p>` → `<p className="text-xs text-text-muted/70">Log output from gadgets will appear here</p>`

### 7. `/Users/jakob/Development/github/jakobwesthoff/torchsnap/src/devtools/console/TreeLogList.tsx`

- **Line 228**: `<p className="text-xs text-text-muted/70">Log output from plugins will appear here</p>` → `<p className="text-xs text-text-muted/70">Log output from gadgets will appear here</p>`

(These two strings are intentionally identical so flat- and tree-view empty states match. Keep them in lockstep.)

### 8. `/Users/jakob/Development/github/jakobwesthoff/torchsnap/src/launcher/Launcher.tsx`

- **Line 452**: `throw new Error("sendMessage called without an active plugin view");` → `throw new Error("sendMessage called without an active gadget view");`

This is a developer-readable error message whose text uses the concept ("plugin view"), not a symbol. Per the discriminator rule, it belongs in P1.

### Files explicitly inspected and intentionally NOT changed in P1

- `src/launcher/main.tsx`, `src/settings/main.tsx` — only comments and import identifiers reference "plugin"; P2/P3.
- `src/launcher/LauncherFooter.tsx`, `src/launcher/visibility.ts`, `src/launcher/hooks/*` — comments only.
- `src/components/{Icon,TitleBar,List}.tsx` — comments and import-from-Tauri-plugin paths only.
- `src/contexts/*` — hook names and error messages name symbols (`<PluginContextProvider>`); P3.
- `src/plugins/clipboard/*` — clipboard-domain copy, no "plugin" word.
- `src/plugins/{registry,types,wasmPluginLoader}.ts` — no JSX, identifiers only.
- `src/devtools/console/{ConsoleToolbar,LogItemRow,formatters,useLogFilters}.tsx/ts` — visible strings are level/source filter labels and rendered `pluginId` values; the `"host"` italic label and the wire-discriminator string `"plugin"` are not user-facing copy in the P1 sense.
- `src/devtools/console/pluginColors.ts` — file rename + identifier rename; P3.
- `src/settings/sections/{GeneralSection,FrecencySection}.tsx` — no visible "plugin" copy; `eventsByPlugin` is a backend-typed key whose visible value is the plugin id string itself (unchanged).
- `src/types.ts`, `src/lib/*`, `src/hooks/*` — identifiers/comments only.
- `launcher.html`, `settings.html`, `devtools.html` — confirmed no "plugin" tokens.

## Commit grouping

Atomic, surface-grouped, no semantic prefix per project convention. Three commits:

1. **Rename "Plugin" to "Gadget" in settings UI copy**
   Files: `src/settings/SettingsSidebar.tsx`, `src/settings/SettingsPanel.tsx`, `src/settings/sections/PluginsManagementPanel.tsx`, `src/settings/sections/WebsiteMetadataSection.tsx`.
   This is the user-visible bulk: sidebar group label, nav item label, the entire management panel (header, descriptions, install/uninstall banners, source-badge tooltips, empty/loading states, file-picker filter name), and the website-metadata description sentence.

2. **Rename "Plugin" to "Gadget" in devtools console empty state**
   Files: `src/devtools/console/LogList.tsx`, `src/devtools/console/TreeLogList.tsx`.
   Both flat and tree empty-state messages flip together.

3. **Rename "plugin view" to "gadget view" in launcher error message**
   File: `src/launcher/Launcher.tsx`.
   Single developer-readable error string. Isolated so QA can revert independently if the concept-word rule turns out to surprise anyone.

Sequence: 1 → 2 → 3 (any order works; sequence above matches descending visibility). All three should land in the same PR/branch but as distinct commits per the atomic-commit rule.

## Test impact

No test-file impact. Verified by `find` against `src/`: no `*.test.ts`, `*.test.tsx`, `*.spec.ts`, `*.spec.tsx`. `package.json` has no `test` script. The Rust suite (`just test-crates`, `just test-plugins`) is unaffected — it tests Rust host and plugin crates, not the TS UI.

No new tests needed. Copy-only changes carry no behavioural risk; the project has no testing-library setup that asserts on rendered text.

## Documentation impact

None. Documentation updates (Plugin-Architecture docs, ADRs, README, control-api.md, etc.) belong to P2. P1 deliberately leaves doc copy stale; the temporary inconsistency between renamed app UI and unrenamed docs is acceptable for the duration between P1 and P2 landing (single-developer pre-alpha context).

## Verification steps

After all three commits land on the branch, run from the repo root:

1. `just check-types` — TypeScript typechecker (`bun run tsc --noEmit`). Must pass with no new errors. Copy-only changes should not introduce any.
2. `just lint-frontend` — eslint (`bun run lint`). Must pass.
3. `just fmt-check-frontend` — prettier check (`bun run fmt:check`). Must pass; if it fails, run `just fmt-frontend` to repair before re-staging.
4. `just check` — full Rust + WIT + WASM + types check, as a paranoia gate (catches accidental import-path damage even though P1 is purely TS-string).
5. **Manual launcher run-through**: `just start` (which wraps `bun run tauri dev`). Verify visually:
   - Settings → sidebar shows "GADGETS" group header (uppercase tracked) instead of "PLUGINS"; "Gadgets" nav entry under General with the puzzle-piece icon.
   - Click into Gadgets section: header reads "Gadgets" with the renamed description; "Installed Gadgets" subsection shows the list; loading state reads "Loading gadgets…"; if empty, "No gadgets registered."
   - Hover badges: "Built-in", "System", "User", "Dev" tooltips all read "…gadget…".
   - Trigger install with a non-`.torchsnap` file (drag-drop): error banner "Only .torchsnap files can be installed as gadgets."
   - Open OS file dialog via "Choose a file…": filter chip reads "Torchsnap Gadget".
   - Uninstall a user gadget: success banner "Uninstalled gadget \"<id>\"."
   - Settings → Website Metadata: description references "gadgets".
   - Open devtools console (whatever shortcut is wired) with no logs flowing: empty state reads "Log output from gadgets will appear here". Switch flat/tree view; both empty states match.
   - Launcher: search/use a gadget normally; visible labels in the launcher itself were already gadget-id-driven and are unchanged.

`just fullcycle` is overkill for a copy-only change but valid as a final pre-PR gate if desired.

## Risks & rollback

**Risks:**

- **Inconsistency window vs. docs.** Until P2 lands, `docs/Plugin-Architecture/*.md`, ADRs, and README still say "Plugin" while the app says "Gadget". Acceptable per inventory (pre-alpha). Document this in the PR description so reviewers don't flag it.
- **Search/grep across the repo** for "plugin" by other developers will turn up fewer hits in the UI layer — this is intended but worth noting in the PR description.
- **Screenshot drift.** Any committed screenshots in `docs/` will be stale. P2's docs rewrite handles them.
- **No string is a wire surface.** Confirmed by re-checking every line above against the inventory: nothing touched is a Tauri command name, event payload key, settings store key, or source-discriminator wire string. The OS file-picker `name: "Torchsnap Plugin"` is dialog chrome, not a wire identifier — `.torchsnap` extension is unchanged.
- **Borderline error message at `Launcher.tsx:452`.** Isolated as commit 3 specifically so it can be reverted alone if the concept-word rule turns out to be controversial. The other three context-hook errors deliberately *do not* land in P1 because they name `<PluginContextProvider>`; they ride P3.

**Rollback:**

- Each commit is independent. To revert any one surface, `git revert <sha>` of the relevant commit.
- Full rollback: `git revert` all three commits (no functional regressions; copy reverts to "Plugin").
- No data-format changes, no migrations, no settings keys touched — rollback is pure text.
