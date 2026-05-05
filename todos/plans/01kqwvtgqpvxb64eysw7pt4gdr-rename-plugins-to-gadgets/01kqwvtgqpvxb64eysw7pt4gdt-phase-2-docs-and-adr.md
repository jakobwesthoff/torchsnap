# Phase 2 — Documentation + ADR

## Summary

P2 records the Plugin → Gadget rename as a new ADR (0042), cross-links it to every ADR that uses plugin terminology via `EDITOR=true adrs link`, rewrites every `.md` file under `docs/` and `todos/` that uses plugin terminology, and renames doc folders/files via `git mv`. Because most architectural docs reference symbols renamed in P3/P4 and paths renamed in P5, P2 is split into two tranches: **P2a** (banner-and-prose work that does not depend on code rename) lands immediately after naming is locked; **P2b** (content rewrites that quote renamed symbols/paths) lands after P5.

## Predecessors / dependencies

- Naming map locked in by `todos/plans/01kqwvtgqpvxb64eysw7pt4gdr-rename-plugins-to-gadgets/inventory.md` (the authoritative reference for every replacement). P2 does not start until inventory.md is final.
- P2a depends only on naming being final.
- P2b depends on P3 (internal Rust/TS rename), P4 (public/wire surface rename), and P5 (repo layout / paths) all being merged, because P2b's code samples and path references quote the post-rename symbols and paths.
- The user-facing string changes in P1 are independent of P2; P2 does not block P1 nor vice-versa.

## Scope

In scope (P2a):

1. New ADR `docs/adr/0042-rename-plugins-to-gadgets.md` (number confirmed: highest current is `0041-zerotier-plugin.md`, verified via `ls docs/adr/`).
2. Cross-link new ADR to every plugin-mentioning ADR via `EDITOR=true adrs link 42 Amends <N> "Amended by"` (single command per target — `adrs link` writes both sides).
3. Touched ADRs: full list below in "ADR cross-link plan".
4. `docs/control-api.md` — single incidental reference ("any active plugin view"); pure prose with no symbol or path quoting.
5. Todos directory renames: `git mv todos/plugin-host todos/gadget-host` and `git mv todos/plugins todos/gadgets`.
6. Todos content audit: 69 `.md` files under `todos/` mention "plugin" (verified `rg -l -i 'plugin' todos --type md | rg -v 'plans/01kqwvtgqpvxb64eysw7pt4gdr-' | wc -l`). Skip files inside the umbrella plan dir.
7. `docs/strategy/Selfcontained-Plugin-System.md` → `Selfcontained-Gadget-System.md` via `git mv` (content rewrite is P2b).
8. Folder rename `git mv docs/Plugin-Architecture docs/Gadget-Architecture` (content rewrite deferred to P2b — see commit grouping).
9. `git rm docs/research/wasm-wit-plugin-system.md` — the file is deleted entirely; it is historical research that has been adopted and need not be carried forward under a new name.

In scope (P2b, lands after P5):

10. README.md (repo root, 78 lines, 23 plugin refs) — terminology audit + update. Quotes P5-renamed paths (`<app_data_dir>/plugins/`, `<app_data_dir>/plugin-home/`, `target/bundled-plugins/`, `plugins/bundled.toml`) and P5-renamed Just recipes (`just build-plugin`, `just check-plugins`); rewrite lands with P5.
11. Project `CLAUDE.md` (93 lines, 21 plugin refs) — terminology audit + update. Quotes P4-renamed macro (`define_plugin!`), P4-renamed SDK crate (`torchsnap-plugin-sdk`), and P5-renamed paths (`plugins/plugin-sdk/`, `plugins/bundled.toml`, `target/bundled-plugins/`, `<app_data_dir>/plugin-home/`); rewrite lands with P5.
12. Content rewrite of all 6 files under `docs/Gadget-Architecture/`, including filename rename `05-plugin-messaging.md` → `05-gadget-messaging.md` via `git mv`.
13. `docs/api/plugin-development.md` → `gadget-development.md` (`git mv` + 1285-line content rewrite, including `define_gadget!`, `torchsnap-gadget-sdk`, `gadgets/gadget-sdk/wit/torchsnap-gadget.wit`, `@torchsnap/gadget-sdk`).
14. `docs/api/logging-system.md` — content audit + update (30 plugin refs).
15. `docs/Howto-build-on-fedora-43.md` — content audit + update (10 plugin refs); quotes P5-renamed paths.

Out of scope (per inventory.md):

- Marketing site (`web/`).
- External Tauri-ecosystem identifiers: `tauri-plugin-store`, `tauri-plugin-global-shortcut`, `@tauri-apps/plugin-*`, Vite's own `Plugin` type.
- User's global `~/.claude/CLAUDE.md`.

## New ADR design

File: `docs/adr/0042-rename-plugins-to-gadgets.md`. Created via `EDITOR=true adrs new "Rename plugins to gadgets"`; the tool generates the file with the next number.

Sections:

- **Title:** `# 42. Rename plugins to gadgets`
- **Date:** today's date (YYYY-MM-DD; `adrs new` fills automatically).
- **Status:** Accepted. Plus the bidirectional `Amends [...]` lines that `adrs link` appends for every touched ADR.
- **Context:** Three short paragraphs.
  - Source: marketing site already uses "Gadgets" as user-facing label — `web/src/site/Header.astro:14` and `web/src/landing/plugin-universe/PluginUniverse.astro:40-42` (file:line, copied verbatim from `todos/01kqw4kqe3g50xn77efwv71jy2-rename-plugins-to-gadgets-everywhere.md:6-12`).
  - Source: pre-alpha status, only consumer is torchsnap itself — inventory.md lines 10–11.
  - Source: prior in-repo terminology was "plugin" across ~33 ADRs, 6 docs/Plugin-Architecture files, ~2178 lines of `.rs`, dozens of `.ts`/`.tsx`. Counts traceable to inventory.md lines 100–110.
- **Decision:** Single sentence: "Rename every in-repo identifier from 'plugin' to 'gadget' across documentation, internal symbols, public API surface, repo layout, and tooling."
  - Sub-bullet: terminology policy. "'Gadget' is the only term going forward in torchsnap. 'Plugin' survives only inside vendored Tauri-ecosystem dependency names (`tauri-plugin-store`, `tauri-plugin-global-shortcut`, `@tauri-apps/plugin-*`) and inside Vite's own exported `Plugin` type." Sources: `Cargo.lock` / `package.json` for the Tauri identifiers; inventory.md lines 92–98 for the locked-in exception list.
  - Sub-bullet: scope exclusions. Marketing site (`web/`) is unchanged in this repo. `manifest.toml` filename remains; only its `[plugin]` section renames to `[gadget]`. App-data directories rename without migration code (existing test installations are wiped manually). Sources: inventory.md lines 12–17.
  - Sub-bullet: archive extension `.torchsnap` is unchanged (product name, not plugin term). Source: inventory.md line 87.
- **Consequences:** Only present-tense facts — what is now true:
  - Every `Plugin*` Rust type, every `plugin*` TS identifier, every WIT package/world/file, every Tauri command name, every settings-key prefix, and every workspace directory now uses Gadget. Sources: the naming-map table in inventory.md lines 24–91.
  - The `manifest.toml` filename retained; its inner `[plugin]` table renamed to `[gadget]`.
  - The `define_plugin!` macro renamed to `define_gadget!`.
  - Vendored Tauri-ecosystem identifiers and Vite's `Plugin` type are unchanged. (Source: same exception block as above.)
  - This ADR amends every prior ADR using plugin terminology — see the bidirectional links inserted by `adrs link`. The architectural decisions in those ADRs are unchanged; only the terminology label is.
- **References:** the original trigger todo at `todos/01kqw4kqe3g50xn77efwv71jy2-rename-plugins-to-gadgets-everywhere.md`, and the shared inventory at `todos/plans/01kqwvtgqpvxb64eysw7pt4gdr-rename-plugins-to-gadgets/inventory.md`.

Validated-document discipline: no "developers will see", no "this enables", no "we plan to". Every claim above traces to a file:line, command output, or a decision recorded in inventory.md. If a section cannot be filled with present-tense facts, drop it.

## ADR cross-link plan

Tool: `adrs link <SOURCE> <LINK> <TARGET> <REVERSE_LINK>`. Convention: `EDITOR=true` (per project; suppresses interactive editor invocation).

Touched ADRs (verified by `grep -l -i plugin docs/adr/*.md`, filtered to remove 0006 and 0007 which only reference external `tauri-plugin-store`):

`0008, 0009, 0010, 0011, 0012, 0013, 0014, 0015, 0016, 0017, 0018, 0019, 0021, 0022, 0023, 0024, 0025, 0026, 0027, 0028, 0029, 0030, 0031, 0032, 0033, 0034, 0035, 0036, 0037, 0038, 0039, 0040, 0041` — 33 ADRs total.

Commands (one per ADR, run from repo root):

```
EDITOR=true adrs link 42 Amends 8  "Amended by"
EDITOR=true adrs link 42 Amends 9  "Amended by"
EDITOR=true adrs link 42 Amends 10 "Amended by"
EDITOR=true adrs link 42 Amends 11 "Amended by"
EDITOR=true adrs link 42 Amends 12 "Amended by"
EDITOR=true adrs link 42 Amends 13 "Amended by"
EDITOR=true adrs link 42 Amends 14 "Amended by"
EDITOR=true adrs link 42 Amends 15 "Amended by"
EDITOR=true adrs link 42 Amends 16 "Amended by"
EDITOR=true adrs link 42 Amends 17 "Amended by"
EDITOR=true adrs link 42 Amends 18 "Amended by"
EDITOR=true adrs link 42 Amends 19 "Amended by"
EDITOR=true adrs link 42 Amends 21 "Amended by"
EDITOR=true adrs link 42 Amends 22 "Amended by"
EDITOR=true adrs link 42 Amends 23 "Amended by"
EDITOR=true adrs link 42 Amends 24 "Amended by"
EDITOR=true adrs link 42 Amends 25 "Amended by"
EDITOR=true adrs link 42 Amends 26 "Amended by"
EDITOR=true adrs link 42 Amends 27 "Amended by"
EDITOR=true adrs link 42 Amends 28 "Amended by"
EDITOR=true adrs link 42 Amends 29 "Amended by"
EDITOR=true adrs link 42 Amends 30 "Amended by"
EDITOR=true adrs link 42 Amends 31 "Amended by"
EDITOR=true adrs link 42 Amends 32 "Amended by"
EDITOR=true adrs link 42 Amends 33 "Amended by"
EDITOR=true adrs link 42 Amends 34 "Amended by"
EDITOR=true adrs link 42 Amends 35 "Amended by"
EDITOR=true adrs link 42 Amends 36 "Amended by"
EDITOR=true adrs link 42 Amends 37 "Amended by"
EDITOR=true adrs link 42 Amends 38 "Amended by"
EDITOR=true adrs link 42 Amends 39 "Amended by"
EDITOR=true adrs link 42 Amends 40 "Amended by"
EDITOR=true adrs link 42 Amends 41 "Amended by"
```

Verification: each touched ADR gains a single new line under `## Status` of the form `Amended by [42. Rename plugins to gadgets](0042-rename-plugins-to-gadgets.md)`. ADR 0042 gains a corresponding `Amends [N. ...]` line per target.

## File-by-file rewrite plan

**P2a (no symbol/path quoting):**

- `docs/control-api.md`: one ref ("any active plugin view"). Replace with "any active gadget view".
- `docs/strategy/Selfcontained-Plugin-System.md` (1152 lines, 271 plugin refs) → `Selfcontained-Gadget-System.md` via `git mv` (in P2a), full content rewrite **deferred to P2b** because the doc quotes renamed symbols/paths.

**P2b (after P5 lands):**

- `README.md` (78 lines, 23 plugin refs): replace user-facing "plugin"/"Plugin" with "gadget"/"Gadget". Update relative links `docs/Plugin-Architecture/01-overview.md` → `docs/Gadget-Architecture/01-overview.md` and `docs/api/plugin-development.md` → `docs/api/gadget-development.md`. Section heading `## Installing plugins` → `## Installing gadgets`; `## Developing plugins` → `## Developing gadgets`; `Settings → Plugins` → `Settings → Gadgets`. Update P5-renamed paths (`<app_data_dir>/plugins/...` → `<app_data_dir>/gadgets/...`, `<app_data_dir>/plugin-home/...` → `<app_data_dir>/gadget-home/...`, `target/bundled-plugins/` → `target/bundled-gadgets/`, `plugins/bundled.toml` → `gadgets/bundled.toml`) and P5-renamed Just recipes (`just build-plugin` → `just build-gadget`, `just check-plugins` → `just check-gadgets`). Keep external Tauri-ecosystem refs unchanged (none in current README).
- `CLAUDE.md` (93 lines, 21 plugin refs): replace plugin terminology. Update "Plugin build pipeline" → "Gadget build pipeline"; "Plugin storage layout" → "Gadget storage layout"; `define_plugin!(MyPlugin)` example → `define_gadget!(MyGadget)`; `torchsnap-plugin-sdk` → `torchsnap-gadget-sdk`; `plugins/plugin-sdk/` → `gadgets/gadget-sdk/`; `plugins/bundled.toml` → `gadgets/bundled.toml`; `target/bundled-plugins/` → `target/bundled-gadgets/`; `<app_data_dir>/plugin-home/` → `<app_data_dir>/gadget-home/`. Keep external Tauri-ecosystem refs unchanged.
- `docs/Gadget-Architecture/01-overview.md` through `06-settings-reactivity.md` (6 files, 263+333+313+347+241+281 = 1778 lines, 419 plugin refs total). Rewrite per naming map. File `05-plugin-messaging.md` → `05-gadget-messaging.md` via `git mv`.
- `docs/api/plugin-development.md` → `gadget-development.md` (`git mv` + 1285-line rewrite). Quotes every renamed symbol from inventory's table, every renamed Tauri command, every renamed WIT path, every renamed npm export.
- `docs/api/logging-system.md` (30 plugin refs): update WIT path, SDK crate name, code samples (`use torchsnap_plugin_sdk::prelude::*` → `use torchsnap_gadget_sdk::prelude::*`), `Plugin(<id>)` log source label.
- `docs/Howto-build-on-fedora-43.md` (10 plugin refs): rewrite under post-P5 paths. Keep external Tauri identifiers (`tauri-plugin-global-shortcut`, `tauri-plugin-store`).
- `docs/strategy/Selfcontained-Gadget-System.md` (renamed in P2a): full content rewrite.

## Folder/file renames

Tracked-file moves use `git mv` (project rule: CLAUDE.md "Use `git mv` for tracked file moves").

P2a:

```
git mv docs/Plugin-Architecture        docs/Gadget-Architecture
git mv docs/strategy/Selfcontained-Plugin-System.md \
       docs/strategy/Selfcontained-Gadget-System.md
git mv todos/plugin-host                todos/gadget-host
git mv todos/plugins                    todos/gadgets
git rm docs/research/wasm-wit-plugin-system.md
```

P2b:

```
git mv docs/Gadget-Architecture/05-plugin-messaging.md \
       docs/Gadget-Architecture/05-gadget-messaging.md
git mv docs/api/plugin-development.md   docs/api/gadget-development.md
```

## Todos audit plan

Method:

1. `git mv todos/plugin-host todos/gadget-host` (single move; sub-tree preserved).
2. `git mv todos/plugins todos/gadgets`.
3. `rg -l -i 'plugin' todos --type md | rg -v 'plans/01kqwvtgqpvxb64eysw7pt4gdr-'` to enumerate target files (~69).
4. Per file, hand-replace per inventory's naming map. Hand-rules:
   - Skip every occurrence of `tauri-plugin-*`, `@tauri-apps/plugin-*`, Vite's exported `Plugin` type.
   - Skip the umbrella plan dir entirely.
   - Treat the original trigger todo `todos/01kqw4kqe3g50xn77efwv71jy2-rename-plugins-to-gadgets-everywhere.md` as historical context; per inventory.md line 320–323, it stays untouched until phases are realised.
5. After audit: `rg -i '\bplugin\b' todos --type md | rg -v 'plans/01kqwvtgqpvxb64eysw7pt4gdr-' | rg -v 'tauri-plugin|@tauri-apps/plugin'` returns only intentional historical refs (e.g. inside the umbrella trigger todo).

The audit does NOT enumerate per-file rewrites. Each todo file is straightforward substring replacement.

## Commit grouping

Atomic commits, each self-contained and buildable (Markdown files always "build"). Project rule: title-only when self-explanatory; body only for non-obvious caveats.

P2a commits:

1. `add ADR 0042 recording plugins-to-gadgets rename` — adds `docs/adr/0042-rename-plugins-to-gadgets.md`.
2. `cross-link ADR 0042 with amended ADRs` — runs the 33 `adrs link` invocations; commit captures all touched ADRs in one go (cross-linking is a single semantic operation).
3. `rename docs/Plugin-Architecture to docs/Gadget-Architecture` — `git mv` only (content rewrite deferred to P2b commit). Body note: "Content rewrite lands after P5 (paths) so code samples reference live identifiers."
4. `rename strategy/Selfcontained-Plugin-System to gadget` — `git mv` only.
5. `rename todos/plugin-host and todos/plugins to gadget variants` — two `git mv` operations.
6. `rewrite plugin terminology in todos` — content audit across todos/.
7. `update docs/control-api.md plugin terminology` — single-line touch.
8. `delete docs/research/wasm-wit-plugin-system.md` — historical research file removed (`git rm`).

P2b commits (after P5 lands):

9. `update README and CLAUDE.md gadget terminology` — both root-level docs; quote P4-renamed symbols (CLAUDE.md) and P5-renamed paths/recipes (README + CLAUDE.md).
10. `rewrite docs/Gadget-Architecture content for gadget terminology` (includes the `05-plugin-messaging.md` → `05-gadget-messaging.md` `git mv`).
11. `rename and rewrite docs/api/plugin-development.md to gadget-development`.
12. `update docs/api/logging-system.md gadget terminology`.
13. `rewrite docs/strategy/Selfcontained-Gadget-System content`.
14. `update docs/Howto-build-on-fedora-43.md gadget terminology`.

## Verification steps

- `rg -i '\bplugin\b' docs/ todos/ README.md CLAUDE.md` after each commit — output must contain only:
  - The umbrella plan directory (intentional skip).
  - External Tauri-ecosystem identifiers (`tauri-plugin-*`, `@tauri-apps/plugin-*`).
  - The original trigger todo `todos/01kqw4kqe3g50xn77efwv71jy2-rename-plugins-to-gadgets-everywhere.md` (historical).
  - Vite's `Plugin` type (none in docs).
  - Any ADR pre-existing prose that intentionally retains "plugin" as historical (none expected after P2b — but the amendment chain via `adrs link` makes the renaming visible without altering historical prose).
- Relative-link audit: every `[…](…)` link inside `docs/` and `README.md` must resolve. The README and `CLAUDE.md` previously linked `docs/Plugin-Architecture/01-overview.md` and `docs/api/plugin-development.md`; both targets renamed.
- `just check-wit` is not part of P2 verification (no WIT changes in this phase).
- After P2a + P2b together: a final `rg -i 'Plugin' docs/adr/*.md` will still return many hits — that is intentional (historical ADR prose is amended, not rewritten; `adrs link` cross-links surface the renaming).

## Risks & rollback

- **Risk: `adrs link` mass-renames touch 33 files; if interrupted mid-run the cross-link state is inconsistent.** Mitigation: run as a single shell loop within one commit; verify with `git status` before committing.
- **Risk: docs/Howto-build-on-fedora-43.md path strings (`plugins/`, `target/bundled-plugins/`) become incorrect once P5 renames the directories.** Choosing P2b for that file avoids stale path strings between P2a and P5.
- **Rollback:** every P2 commit is independent and reversible via `git revert`. The new ADR's `adrs link` cross-links can be reverted by reverting the cross-link commit; the touched ADRs return to their pre-P2 form. If P2b's content rewrites turn out to need adjustment, revert just the affected commit and re-do — no ABI/wire surface affected.

## Critical files

- `/Users/jakob/Development/github/jakobwesthoff/torchsnap/docs/adr/0041-zerotier-plugin.md` (last existing ADR, defines the structural template for ADR 0042)
- `/Users/jakob/Development/github/jakobwesthoff/torchsnap/docs/adr/0012-use-prefix-based-exclusive-routing-for-query-plugins.md` (canonical example of `Amended by` cross-link inserted by `adrs link`)
- `/Users/jakob/Development/github/jakobwesthoff/torchsnap/todos/plans/01kqwvtgqpvxb64eysw7pt4gdr-rename-plugins-to-gadgets/inventory.md` (the naming map every rewrite consults)
- `/Users/jakob/Development/github/jakobwesthoff/torchsnap/README.md` (forward-facing entry point; relative links to renamed folders)
- `/Users/jakob/Development/github/jakobwesthoff/torchsnap/CLAUDE.md` (project conventions; quotes renamed symbols and paths)
