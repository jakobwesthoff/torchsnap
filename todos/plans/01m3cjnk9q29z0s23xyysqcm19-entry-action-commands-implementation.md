---
kind: plan
status: open
---

# Implementation plan: entry action commands and fixed slots

This plan implements ADR 55
(`docs/adr/0055-dispatch-entry-actions-through-gadget-commands-in-fixed-slots.md`)
and ADR 56
(`docs/adr/0056-version-the-wit-contract-and-both-gadget-sdks-in-lockstep.md`),
and coordinates these todos:

| Todo | Phase |
|---|---|
| `todos/gadget-host/api/01kr2357mcz36g4gte0c0t1qz5-entry-action-commands-and-slots.md` | 5 to 7 |

The ADRs hold the design decisions. The todos hold the per-topic
details: the gadget migration table, the `open` to `primary` rename
list and the docs pages. This plan fixes the order, the commits and the
checks. Tick the boxes as the work progresses.

## Ground rules

- Work on the branch `entry-action-commands` in the worktree
  `../torchsnap.worktrees/entry-action-commands`.
- Phases 1 to 4 end every commit with a green `just fullcycle`. Commits
  inside phase 5 may leave the build broken; the maintainer allowed
  that for the WIT switch. The branch tip passes `just fullcycle`
  before the merge.
- Merge into `main` with `git merge --no-ff`, so
  `git bisect --first-parent` on `main` skips the broken commits. The
  branch is merged twice: after phase 4, so the fixes reach `main`
  early, and after phase 7.
- Phase 6's torchsnap-docs changes go on a branch of the same name in
  torchsnap-docs, merged into its `main` and pushed right after the
  code is on `main`.
- Bugs are test-first: a regression test that fails, then the fix.
  Every changed path gets tests, including error and edge cases
  (`CLAUDE.md`, "Tests and quality gates").
- Every user-visible change gets a `CHANGELOG.md` entry under
  `[Unreleased]`, in the commit that makes the change.
- When a todo is done, delete it and every reference to it
  (`rg <ulid> . ../torchsnap-docs ../torchsnap-web`), copying what a
  referencing file still needs into it.
- Set a todo's `status` to `in-progress` when its phase starts.

## Phase 0: setup

- [x] `git worktree add ../torchsnap.worktrees/entry-action-commands -b entry-action-commands main`
- [x] Baseline `just fullcycle` in the worktree. If it fails, stop and
  report what fails before changing anything.

## Phase 1: generation guard for the entry store

Done: `EntryStore` drops results of an older search. Must land before
phase 5, because with commands a stale entry runs a stale command.

- [x] Failing tests in `src-tauri/src/entry_store.rs`: a new generation
  clears the store; an insert for the current generation is stored; an
  insert for an older generation is dropped; two overlapping searches
  leave only the newer one's entries.
- [x] Give `EntryStore` a generation: starting a search clears the map
  and returns a new generation, and `insert` takes the generation it
  was produced for. Check and insert happen under the same write lock.
- [x] `GadgetHost::search` takes the generation at the top and passes
  it to every `store_sourced_entries` call.
- [x] `CHANGELOG.md`, Fixed.
- [x] Delete the todo, remove its `depends-on` entry from
  `01kr2357mcz36g4gte0c0t1qz5` and its row in this plan's table, and
  update the other references.
- [x] `just fullcycle`, commit.

## Phase 2: calculator copy on Enter

Done: the views copy through a `copy` message, then `dismiss()`. Uses
the ADR 55 rule for views: a view acting on its own state calls
`sendMessage`, then a launcher action.

- [x] Add gadget view tests to the vitest run: extend `include` in
  `vitest.config.ts` with the gadget frontends, make the
  `@torchsnap/gadget-sdk/*` shims resolve, and call
  `setupSdkGlobalsForTesting()` from `packages/gadget-sdk/src/testing/`.
  Commit this on its own once a trivial gadget view test passes.
- [x] Failing tests with `MockGadgetContextProvider`: Enter in
  `CalculatorInline`, Enter in `CalculatorView` (current result and
  selected history row), and a click on a history row each call
  `sendMessage("copy", { expression, result, resultType })` and then
  `dismiss()`.
- [x] Calculator backend: a `copy` message that writes the result with
  `clipboard::write_text` and saves the history entry. Unit-test its
  payload decoding (host imports cannot run in host-target tests).
  Remove `save_history` if nothing calls it anymore.
- [x] Views: `await sendMessage("copy", …)`, then `dismiss()`. No more
  `onExecute` with a result string, no separate `save_history`.
- [ ] Manual check in the app: `=2+2` and Enter, `2+2` inline and
  Enter, click on a history row. Each copies and closes the launcher.
  Left to the maintainer: the automated run cannot drive the app.
- [x] `CHANGELOG.md`, Fixed.
- [x] Delete the todo. Its path is cited in ADR 55's Context and in
  `01kr2357mcz36g4gte0c0t1qz5`: keep the facts, drop the path.
- [x] `just fullcycle`, commit.

## Phase 3: `openSettings()` for views

Additive, from ADR 55: every launcher effect of a WIT post-action is
also a `LauncherActions` function.

- [x] Tauri command, for example `gadget_open_settings(gadget_id)`,
  that hides the launcher and calls `show_settings_window_at(app,
  gadget_id)`, the same effect as the `open-settings` post-action
  (decided in ADR 55).
  Register it and type it in `src/lib/command.ts`.
- [x] `openSettings()` in `LauncherActions` in
  `src/contexts/GadgetContext.tsx` and
  `packages/gadget-sdk/src/shims/hooks.ts`, wired in
  `src/launcher/Launcher.tsx` for custom views and inline views with
  the view's gadget id. Add it to `MockGadgetContextProvider`.
- [x] Tests with `mockIPC`: calling `openSettings()` invokes the
  command with the view's gadget id. If rendering `Launcher` is too
  heavy for a test, extract the construction of the launcher actions
  into a function and test that.
- [x] `CHANGELOG.md`, Added (for gadget authors).
- [x] `just fullcycle`, commit.

## Phase 4: typed messaging

Done: `Messaging` in `gadgets/gadget-sdk/src/messaging.rs`, used by
bangs, calculator, template and zerotier. A `{}` payload is decoded as
given first and without the payload only when that fails, so both unit
variants and `Variant {}` accept it.

- [x] SDK: `Messaging` trait with `type Request` and a blanket
  `MessagingGuest` impl in `gadgets/gadget-sdk/src/messaging.rs`. A
  `{}` payload is treated as no payload. Tests on the host target:
  known method, unknown method, payload mismatch, `{}` for a unit
  variant, response encoding.
- [x] Prelude exports the `Messaging` trait. `parse_payload` and
  `to_response` stay in the module but leave the prelude.
- [x] Convert bangs, calculator (including the new `copy`), template
  and zerotier. Adapt their tests.
- [x] Delete the todo; ADR 55 cites its path under "Rust gadget SDK":
  keep the facts, drop the path.
- [x] `just fullcycle`, commit (SDK and gadgets may be two commits).

## Checkpoint: merge phases 1 to 4

- [x] `just fullcycle` on the branch tip.
- [x] Merge the branch into `main` with `--no-ff`. Push `main` after
  the maintainer confirms.
- [x] Keep working on the same branch for phases 5 to 7.

## Phase 5: the switch

Todo `01kr2357mcz36g4gte0c0t1qz5` holds the details for every step
here. Commits in this phase may not build. Commit after each step.

- [x] **5.1 WIT and versions.** New `action` and `entry-actions`
  records in `catalog-entry` and `scored-entry`, `execute(command:
  string)`, no `scored-entry.data`, no `action-id`. Package version
  `0.2.0`; `gadgets/gadget-sdk/Cargo.toml` and
  `packages/gadget-sdk/package.json` to `0.2.0` (ADR 56). `just
  check-wit`.
- [x] **5.2 Rust SDK.** `Search` trait, SDK-owned generic entry types,
  `Actions<C>` with builder (label required for `primary` and
  `secondary`) and `iter()`, blanket `SearchGuest`. Decode failure in
  `execute()` returns `Err`; encode failure logs a warning and drops
  the entry. Prelude swap; `data` becomes internal. Tests on the host
  target for encode, decode and the error paths.
- [x] **5.2a Blanket impl check.** Convert template first and build it
  for `wasm32-wasip2`. If the blanket impl does not work with the
  `wit_bindgen` `export!`, stop and bring it to the maintainer.
- [x] **5.3 WASM gadgets.** The remaining seven, per the migration
  table. hello-world gets `clipboard = true` and really copies. `just
  check-gadgets`, `just test-gadgets`.
- [x] **5.4 Test fixtures.** Update the six fixtures in
  `src-tauri/tests/fixtures/` (hand-written `SearchGuest` against raw
  `wit_bindgen`), `just build-test-fixtures`, commit the `.wasm` files.
- [x] **5.5 Host.** Slot types, `EntryActions<C>`, generic entry and
  response types, `ErasedSearch` and typed `Search` with blanket impl,
  `Gadget: ErasedSearch`. Actions serialize to the frontend as `{ slot,
  label }` in slot table order, without commands. `search_execute`
  takes `(source, entry_id, slot)`; an empty slot logs and returns
  `Nothing`. WIT conversions in `src-tauri/src/wasm/bindings.rs`,
  `bridge.rs` and `runtime/instance.rs`; `default_keybinding_for`
  goes away. Tests: commands never serialized, list order, empty slot,
  blanket encode and decode, conversions.
- [x] **5.6 Native gadgets.** app_launcher, clipboard, commands,
  system_preferences, system_commands on `Search`. App launcher's
  Reveal goes to `secondary`. Adapt their tests.
- [ ] **5.7 Frontend.** One slot table (keys, default labels, title
  fallback for `primary` and `secondary`). Slot types in `src/types.ts`
  and `packages/gadget-sdk/src/types/data.ts`. Footer, `executeEntry`,
  row click and keyboard navigation use the slot table and the
  `primary` slot. `onExecute(entryId, slot)` in both `LauncherActions`
  definitions and the launcher wiring. Gadget views: emoji-picker grid
  and calculator history rows call `onExecute(entry.id, "copy")`.
  Tests for the slot table, the footer derivation and the key
  bindings.
- [ ] **5.8 Green.** `just fullcycle` passes on the branch.
- [ ] **5.9 Checks,** report the results to the maintainer:
  - What a gadget built against `0.1.0` reports when loaded, for
    example a fixture `.wasm` from `main`.
  - The cost of encoding commands in `entries()`, which runs on every
    search without a prefix, for app_launcher with its installed apps.
    No caching unless the maintainer decides so.
- [ ] `CHANGELOG.md`: Changed (gadget API `0.2.0`: slots, commands,
  `execute(command)`), Removed (`custom`, `open-with`, `data`), Added
  (`secondary` slot on Cmd+Enter), plus anything else user-visible.

## Phase 6: documentation

- [ ] In this repository: `docs/api/gadget-development.md`,
  `docs/Gadget-Architecture/01-overview.md` to `06-settings-reactivity.md`
  where they describe actions, `execute()`, `data` or the version, the
  crate docs at the top of `gadgets/gadget-sdk/src/lib.rs`, and the
  feature list in `gadgets/template/src/lib.rs`.
- [ ] In torchsnap-docs, following its `CLAUDE.md` and
  `docs/documentation-writing-howto.md`: `development/search.mdx`,
  `development/interfaces/index.mdx` (version note),
  `development/interfaces/imports.mdx`,
  `development/interfaces/exports.mdx`,
  `development/your-first-gadget.mdx`,
  `development/frontend/messaging.mdx`,
  `development/frontend/views.mdx`. Work on the `entry-action-commands`
  branch there. `just fullcycle` there, commit.

## Phase 7: landing

- [ ] `just fullcycle` on the branch tip.
- [ ] Delete todo `01kr2357mcz36g4gte0c0t1qz5` and its references;
  ADR 55 and ADR 56 keep their facts.
- [ ] Merge into `main` with `--no-ff`. Push `main` after the
  maintainer confirms.
- [ ] Right after the code is on `main`: merge the torchsnap-docs
  branch into its `main` and push it.
- [ ] Sweep all todos for work that is now done, with Sonnet or Haiku
  agents (`CLAUDE.md`, "Todos").
- [ ] Remove the worktree and delete this plan.
