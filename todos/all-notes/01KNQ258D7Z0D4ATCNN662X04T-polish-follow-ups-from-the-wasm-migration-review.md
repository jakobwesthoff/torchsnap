---
title: "Polish follow-ups from the WASM migration review"
kind: improvement
component: gadget-host
impact: low
origin: review
tags: [testing, error-handling, logging]
---
# Polish follow-ups from the WASM migration review

Polish items surfaced by the post-migration validation review of 2026-04-08 (seven parallel review agents covering D1-D6 and the calculator port). The critical and important findings were fixed in 11 follow-up commits (clipboard host import, SQL handle drop, scheduler abort race, lazy SDK shim resolution, type sync assertion, etc.). The items below are the lower-priority polish that was deliberately deferred. None of them blocks current functionality. They suit incremental pickup rather than a single sweep. Each item keeps the original review section number so back-references stay intact.

## #22: Explanatory comments on intentionally omitted memo deps

**Where:** `src/launcher/Launcher.tsx`, around the `gadgetLauncher` and `inlineLauncher` `useMemo` blocks (around lines 498 and 521 in the slice-assembly section; line numbers drift, so search for `useMemo<LauncherActions>`).

**The thing:** the deps arrays for these memos intentionally omit `setGadgetFooter` / `setInlineFooter` (stable React `setState` setters) and `mouseActiveRef` (a stable ref). The omissions are correct but undocumented. A future maintainer running an exhaustive-deps lint check will be tempted to "fix" them and trigger needless re-renders.

**Fix:** add a one-line comment above each memo deps array explaining why those values are stable and absent on purpose.

## #23: Generalize the `useLauncher()` error message

**Where:** `src/contexts/useLauncher.ts` and the matching shim in `gadget-sdk/src/shims/hooks.ts`.

**Current error string:**
> `useLauncher called outside the launcher tree (settings panels do not have launcher actions)`

**The thing:** the parenthetical is too specific. Today the only non-launcher mount is the settings panels, but the GadgetContext design supports future mount kinds (modal slot, command palette, menu bar and so on). When those land, the error will be misleading.

**Fix:** drop the settings-panel name. Suggested: `useLauncher called from a context that was not mounted inside the launcher tree: this hook is only available to launcher view and inline-view components`.

## #24: Context test scaffolding

**Where:** `src/contexts/__tests__/` (the directory does not exist yet).

**The thing:** the entire `src/contexts/` gadget component infrastructure ships without unit tests. The migration plan called for "extensive test coverage" of every new behaviour. This is the largest gap.

**Concrete tests to add:**

- **`useLauncher.test.tsx`**: `useLauncher()` throws with the exact expected message when called (a) outside any provider and (b) inside a `GadgetContextProvider` with no `launcher` slice. Use `renderHook` and `expect(() => ...).toThrow(...)`.
- **`useGadgetSetting.test.tsx`**: `useGadgetSetting("foo")` inside a provider with `id: "calculator"` reads `gadgets.calculator.foo` from the underlying store. This verifies the namespace prefixing invariant the entire settings contract depends on.
- **`GadgetContextProvider.test.tsx`**: a child calling the legacy `useLogger()` inside `GadgetContextProvider` receives the same `Logger` instance as `useGadgetRuntime().logger`. This guards the dual-drive coupling that was deliberately kept when the new contract was built.
- **`Launcher.test.tsx`** (a new file, or folded into an existing Launcher test): the `useOptionalGadgetEnabled` path with `customGadgetView == null` resolves to `false` without reading any real `enabled.<gadget-id>` key.

**Blocker for landing this:** the host frontend has no test infrastructure yet. Setting up Vitest and React Testing Library is a small but real prerequisite. The plan's "Frontend SDK testing requirements" section gives the shape. `@torchsnap/gadget-sdk/testing`'s `MockGadgetContextProvider` already exists and would be the basis for these tests.

## #25: Workspace tsconfig path alias for `@torchsnap/host/*`

**Where:** `gadget-sdk/tsconfig.json` (does not exist yet), `gadget-sdk/src/testing/setup.ts`, `gadget-sdk/src/testing/MockGadgetContextProvider.tsx`.

**The thing:** the testing entry point imports host source through relative paths:

```ts
import { initGadgetSdk } from "../../../src/lib/sdk";
import { GadgetContext } from "../../../src/contexts/GadgetContext";
```

These work inside the monorepo but break as soon as the SDK is published to npm or moved out of the workspace. `setup.ts` documents the deferral, but the relative paths are still spread across two files.

**Fix:** add a `gadget-sdk/tsconfig.json` with a `paths` mapping `"@torchsnap/host/*": ["../../src/*"]`, and rewrite the two imports to `import ... from "@torchsnap/host/lib/sdk"` and `"@torchsnap/host/contexts/GadgetContext"`. The intent ("reaching into host source") becomes structural rather than visual.

When the SDK is published, swap the path alias for a real `@torchsnap/host` peer dependency or a separate `@torchsnap/host-fixtures` package. Only the tsconfig entry changes, not every file that uses it.

## #26: `MockGadgetContextProvider` JSDoc accuracy

**Where:** `gadget-sdk/src/testing/MockGadgetContextProvider.tsx`.

**The thing:** the `launcher` prop's JSDoc says "calling `useLauncher()` from a child throws exactly as it would in a settings panel". That is accurate for the current code, but it conflates two facts: (a) the `launcher` slice is absent, and (b) `useLauncher()` throws when the slice is absent. If `useLauncher()` ever returns `undefined` instead of throwing, the doc claim silently becomes wrong.

**Fix:** describe the slice presence directly, not the consequent throw. For example: "When `launcher` is omitted, the rendered context value carries no `launcher` slice. Calling `useLauncher()` from a child fails according to what the host's hook does in that case (currently it throws)."

A trivial doc-only change.

## #27: Architecture doc example uses raw `unwrap_or_else`

**Where:** `docs/Gadget-Architecture/06-settings-reactivity.md`, lines around 109 to 111.

**The thing:** the inline example reads a WASM setting as:

```rust
serde_json::from_str(
    &settings::get("verbose").unwrap_or_else(|| "false".into())
)
```

It is verbose and slightly misleading. The template gadget and the calculator both use small `read_bool_setting` / `read_string_setting` helpers. New gadget authors reading the architecture doc will copy the verbose form.

**Fix:** either replace the example with the helper form (and refer to the Rust SDK crate), or add a one-line note under the example pointing at the helper idiom in `gadgets/template/src/lib.rs`. The helper form is cleaner and reflects what real gadgets do.

## #28: Clearer `SqlConfig::None` error message

**Where:** `src-tauri/src/wasm/runtime.rs`, in `bindings::torchsnap::gadget::sql::Host::open` (around the `SqlConfig::None` arm).

**Current error:** `gadget has no [storage.sql] declared in manifest.toml`

**The thing:** accurate, but it misses the case where the gadget has a `[storage]` table and forgot the `[storage.sql]` sub-table, or has `[storage.sql]` with an empty `migrations` key.

**Fix:** rephrase so the message names the action:

```
gadget has no [storage.sql] block declared in its manifest.toml: add a [storage.sql] migrations = [...] entry to enable SQL storage
```

A trivial string change.

## #29 and #33: Visibility for `query_history` errors in calculator

**Where:** `gadgets/calculator/src/lib.rs`, the `query_history` function.

**Current behaviour:** `db.query(...)` errors are silently swallowed (`Err(_) => return vec![]`), which produces an empty history list with no trace in the dev tools.

**Fix:** replace the swallow with a `logging::log` call so query failures appear in the host's devtools console:

```rust
let rows = match db.query(&sql_text, &params) {
    Ok(r) => r,
    Err(e) => {
        logging::log(
            logging::LogLevel::Warn,
            &format!("calculator history query failed: {e}"),
            &[],
            None,
        );
        return vec![];
    }
};
```

The fallback to an empty list is correct (search should not crash on a failed history query). The failure just has to be visible.

The same defensive logging should be added where `expect_text`'s filter_map drops a row with a non-Text column, though that path is currently unreachable given the schema.

## #30: Template `enable_log` table mixes two event types

**Where:** `gadgets/template/migrations/001_init.sql` and `gadgets/template/src/lib.rs` (`enable()` and the `heartbeat` task).

**The thing:** the `enable_log` table is written from two places: the lifecycle `enable()` and the scheduled `heartbeat` task. No column tells the two event types apart. Gadget authors reading the demo see a table called `enable_log` holding both kinds of events and get confused.

**Fix:** add a `reason TEXT NOT NULL DEFAULT 'enable'` column to the migration. The `enable()` insert keeps the default, and the `heartbeat` task inserts with `reason = 'heartbeat'` explicitly. The frontend can then count each type with `SELECT COUNT(*) FROM enable_log WHERE reason = ?`.

This is a small but pedagogically valuable change. It shows gadget authors the right pattern for multi-event log tables instead of teaching them to mix unrelated events into one undifferentiated table.

**Migration concern:** the template is a greenfield example with no production databases, so this can be a destructive edit to `001_init.sql` rather than a `002_*.sql` add-column migration. Real gadget authors should add a new migration file for production schemas, and the template should call this out in a comment.

## #31: Friendlier rejection of Quartz `@daily` and `@hourly` macros

**Where:** `src-tauri/src/wasm/manifest.rs`, `parse_cron_schedule` (after the existing 5-field token-count check that landed in 3ecc17c).

**The thing:** gadget authors who write `@daily` or `@hourly` (Quartz aliases for common schedules) get a generic "expected 5-field POSIX cron, got 1 field(s)" error. The 1-field count is technically correct but not actionable: the author thinks `@daily` is a field, not an alias they should expand.

**Fix:** detect single-token inputs that start with `@` and emit a specific error that suggests the POSIX equivalent:

```rust
if let Some(macro_name) = schedule.strip_prefix('@') {
    let suggestion = match macro_name {
        "yearly" | "annually" => Some("0 0 1 1 *"),
        "monthly" => Some("0 0 1 * *"),
        "weekly" => Some("0 0 * * 0"),
        "daily" | "midnight" => Some("0 0 * * *"),
        "hourly" => Some("0 * * * *"),
        _ => None,
    };
    if let Some(equiv) = suggestion {
        anyhow::bail!(
            "Quartz `@{macro_name}` aliases are not supported. Use the equivalent 5-field POSIX expression `{equiv}`"
        );
    }
}
```

Add tests `@daily_rejected_with_hint` and `@hourly_rejected_with_hint` covering the alias path. The existing `quartz_macro_at_daily_rejected` test already verifies that `@daily` fails. This change only makes the message more useful.

## #32: Typed payload for calculator `save_history_method`

**Where:** `gadgets/calculator/src/lib.rs`, `save_history_method`.

**Current pattern:** three chained `value.get("...").and_then(...).ok_or("missing '...'")?` calls extract `expression`, `result` and `resultType`.

**The thing:** the form is correct but verbose, and the error messages are static literals with no context about what was received. A `#[derive(serde::Deserialize)]` struct would be cleaner and give better errors.

**Fix:** add a typed request struct:

```rust
#[derive(serde::Deserialize)]
struct SaveHistoryRequest {
    expression: String,
    result: String,
    #[serde(rename = "resultType")]
    result_type: String,
}

let req: SaveHistoryRequest = serde_json::from_str(payload)
    .map_err(|e| format!("invalid save_history payload: {e}"))?;
```

This requires adding `serde` as a direct dependency of the calculator crate (only `serde_json` is listed now; the derive macro needs `serde`).

The Rust gadget SDK (`torchsnap-gadget-sdk`) now exposes `messaging::parse_payload<T: DeserializeOwned>` and `messaging::to_response<T: Serialize>` for exactly this pattern, and the calculator's `save_history_method` is already migrated to them. If any remaining handler still parses manually, apply the SDK helpers rather than open-coding another `serde_json::from_str`.

## Out-of-scope items still tracked elsewhere

These came up in the review but already have dedicated todos or were explicitly deferred by the migration plan:

- **Rust gadget SDK crate**: implemented as `torchsnap-gadget-sdk` (see `gadgets/gadget-sdk/`). Gadgets consume it through `use torchsnap_gadget_sdk::prelude::*;`, and the `define_gadget!` macro replaces the hand-rolled `wit_bindgen::generate!` and `export!` boilerplate.
- **Dedicated `gadgets/test-fixture/` crate** for end-to-end WASM bridge integration tests against the wasmtime linker. The migration plan required it for the D1-D4 layered tests, but it was never created. It would unblock the SQL storage tests the calculator port had to skip (`history_save_and_query`, `history_dedup_bumps_timestamp`, `history_filter`).
- **`spawn_blocking` for the scheduler's `run_task` call**: covered by a separate todo (see Relations). The scheduler's inline comment now documents the threshold (`scheduler_loop` in `src-tauri/src/wasm/bridge.rs`).
- **`messaging-stream` WIT sub-interface** for WASM gadget streaming RPC. Out of scope per ADR 0030. It would also need the gadget channel lifecycle todo (see Relations) to land first.

## Suggested PR slicing when this lands

These items are independent and small. Cluster them by area to keep diffs reviewable:

1. **Frontend polish PR**: #22, #23, #26
2. **Rust polish PR**: #28, #29
3. **Docs polish PR**: #27
4. **Template DX PR**: #30, #31 (the friendlier cron error for `@daily` macros has gadget-author user impact, so it is worth combining with the template `reason` column change)
5. **Test scaffolding PR**: #24 and #25 (the workspace tsconfig alias is a prerequisite for the testing entry points, and the context tests are the first real users of `MockGadgetContextProvider`)
6. **Defer until the Rust SDK crate lands**: #32

No single PR is large enough to justify slicing further. The test scaffolding one is the biggest because it requires setting up Vitest and React Testing Library from scratch on the host frontend.

## Relations

- Relates to: [Use spawn_blocking instead of std::thread::spawn for Tokio-dependent threads](01KN89G33581ZA71V2JBBHX4AH-use-spawnblocking-instead-of-stdthreadspawn-for-tokio-dependent-threads.md), which covers the scheduler's `run_task` call.
- Relates to: [Add explicit close signaling for gadget channels](01KN30TH27X0ARV9A5CEKKWGPW-add-explicit-close-signaling-for-gadget-channels.md), which the messaging-stream item needs first.
