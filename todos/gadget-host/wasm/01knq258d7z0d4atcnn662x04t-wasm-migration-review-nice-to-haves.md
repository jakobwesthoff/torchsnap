---
kind: improvement
status: open
---

# WASM migration review — nice-to-have follow-ups

Polish items surfaced by the post-migration validation review
(2026-04-08, seven parallel review agents covering D1-D6 + the
calculator port). The critical and important findings were
fixed in 11 follow-up commits (clipboard host import, SQL handle
drop, scheduler abort race, lazy SDK shim resolution, type sync
assertion, etc.) — the items below are the lower-priority polish
that we deliberately deferred. Each is tagged with the original
review section number so back-references stay intact.

None of these block any current functionality. They are quality
nudges that make sense to pick up incrementally rather than as a
single sweep.

---

## #22 — Explanatory comments on intentionally-omitted memo deps

**Where:** `src/launcher/Launcher.tsx` around the `gadgetLauncher`
and `inlineLauncher` `useMemo` blocks (currently around lines
498 and 521 in the slice-assembly section, but line numbers
drift — search for `useMemo<LauncherActions>`).

**The thing:** the deps arrays for these memos intentionally
omit `setGadgetFooter` / `setInlineFooter` (stable React
`setState` setters) and `mouseActiveRef` (a stable ref). The
omissions are correct but undocumented; a future maintainer
running an exhaustive-deps lint check will be tempted to
"fix" them and trigger needless re-renders.

**Fix:** add a one-line comment above each memo deps array
explaining why those values are stable and absent on purpose.

---

## #23 — Generalize `useLauncher()` error message

**Where:** `src/contexts/useLauncher.ts` and the matching shim
in `gadget-sdk/src/shims/hooks.ts`.

**Current error string:**
> `useLauncher called outside the launcher tree (settings panels do not have launcher actions)`

**The thing:** the parenthetical is too specific. Today the
only non-launcher mount is settings panels, but the
GadgetContext design supports future mount kinds (modal slot,
command palette, menu bar, etc.). When those land the error
will be misleading.

**Fix:** rephrase to drop the settings-panel name. Suggested:
`useLauncher called from a context that was not mounted inside the launcher tree — this hook is only available to launcher view and inline-view components`.

---

## #24 — Context test scaffolding

**Where:** `src/contexts/__tests__/` (directory does not yet exist).

**The thing:** the entire `src/contexts/` gadget component
infrastructure ships without unit tests. The migration plan
called for "extensive test coverage" of every new behavior;
this is the largest gap.

**Concrete tests to add:**

- **`useLauncher.test.tsx`** — `useLauncher()` throws with the
  exact expected message when called (a) outside any provider
  and (b) inside a `GadgetContextProvider` with no `launcher`
  slice. Use `renderHook` + `expect(() => ...).toThrow(...)`.
- **`useGadgetSetting.test.tsx`** — `useGadgetSetting("foo")`
  inside a provider with `id: "calculator"` reads
  `gadgets.calculator.foo` from the underlying store. Verifies
  the namespace prefixing invariant the entire settings
  contract depends on.
- **`GadgetContextProvider.test.tsx`** — a child calling the
  legacy `useLogger()` inside `GadgetContextProvider` receives
  the same `Logger` instance as `useGadgetRuntime().logger`.
  Guards the dual-drive coupling we explicitly preserved when
  building the new contract.
- **`Launcher.test.tsx`** (new file or fold into an existing
  Launcher test) — the `useOptionalGadgetEnabled` code path
  with `customGadgetView == null` resolves to `false` without
  reading any real `enabled.<gadget-id>` key.

**Blocker for landing this:** the host frontend has zero
existing test infrastructure. Setting up Vitest + React
Testing Library is a small but real prerequisite — see the
plan's "Frontend SDK testing requirements" section for the
shape (`@torchsnap/gadget-sdk/testing`'s
`MockGadgetContextProvider` already exists and would be the
basis for these tests).

---

## #25 — Workspace tsconfig path alias for `@torchsnap/host/*`

**Where:** `gadget-sdk/tsconfig.json` (does not yet exist),
`gadget-sdk/src/testing/setup.ts`,
`gadget-sdk/src/testing/MockGadgetContextProvider.tsx`.

**The thing:** the testing entry point currently imports from
host source via relative paths:

```ts
import { initGadgetSdk } from "../../../src/lib/sdk";
import { GadgetContext } from "../../../src/contexts/GadgetContext";
```

These work in-monorepo but break the moment the SDK is
published to npm or moved out of the workspace. The setup.ts
file documents the deferral but the relative paths are still
sprinkled across two files.

**Fix:** introduce a `gadget-sdk/tsconfig.json` with a `paths`
mapping `"@torchsnap/host/*": ["../../src/*"]` and rewrite the
two imports to `import ... from "@torchsnap/host/lib/sdk"` /
`"@torchsnap/host/contexts/GadgetContext"`. The semantic intent
("I'm reaching into host source") becomes structural rather
than visual.

When the SDK eventually publishes, swap the path alias for a
real `@torchsnap/host` peer dependency or a separate
`@torchsnap/host-fixtures` package — only the tsconfig entry
changes, not every file that uses it.

---

## #26 — `MockGadgetContextProvider` JSDoc accuracy

**Where:**
`gadget-sdk/src/testing/MockGadgetContextProvider.tsx`.

**The thing:** the `launcher` prop's JSDoc says
*"calling `useLauncher()` from a child throws exactly as it
would in a settings panel"*. That's accurate for the
current code, but it conflates two distinct facts: (a) the
`launcher` slice is absent, and (b) `useLauncher()` throws
when the slice is absent. If we ever change `useLauncher()`
to return `undefined` instead of throwing, the doc claim
becomes silently wrong.

**Fix:** rephrase to describe the slice presence directly,
not the consequent throw. Something like: "When `launcher`
is omitted, the rendered context value carries no
`launcher` slice — calling `useLauncher()` from a child
will fail according to whatever the host's hook does in
that case (currently throws)."

Trivial doc-only change.

---

## #27 — Architecture doc example uses raw `unwrap_or_else`

**Where:**
`docs/Gadget-Architecture/06-settings-reactivity.md` lines
~109-111.

**The thing:** the inline example shows the WASM settings
read pattern as:

```rust
serde_json::from_str(
    &settings::get("verbose").unwrap_or_else(|| "false".into())
)
```

That's verbose and slightly misleading — the template gadget
and the calculator both use small `read_bool_setting` /
`read_string_setting` helpers instead. New gadget authors
reading the architecture doc will cargo-cult the verbose form.

**Fix:** either replace the example with the helper-style
form (and forward-reference the future Rust SDK crate todo),
or add a one-line note at the bottom of the example pointing
at the helper idiom in `gadgets/template/src/lib.rs`. The
helper form is cleaner and reflects what real gadgets do.

---

## #28 — Better `SqlConfig::None` error message

**Where:** `src-tauri/src/wasm/runtime.rs` in
`bindings::torchsnap::gadget::sql::Host::open` (around the
`SqlConfig::None` arm).

**Current error:** `"gadget has no [storage.sql] declared in manifest.toml"`

**The thing:** accurate but misses the case where the gadget
HAS a `[storage]` table but forgot the `[storage.sql]`
sub-table, or has `[storage.sql]` with an empty `migrations`
key.

**Fix:** rephrase to suggest the action:

```
gadget has no [storage.sql] block declared in its manifest.toml — add a [storage.sql] migrations = [...] entry to enable SQL storage
```

Trivial string change.

---

## #29 / #33 — Visibility for `query_history` errors in calculator

**Where:** `gadgets/calculator/src/lib.rs query_history`
function.

**Current behavior:** `db.query(...)` errors are silently
swallowed (`Err(_) => return vec![]`), producing an empty
history list with no clue in dev tools.

**Fix:** replace the swallow with a `logging::log` call so
query failures appear in the host's devtools console:

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

The fallback to empty list is correct (search shouldn't crash
on a failed history query) — just make the failure visible.

The same defensive logging should be added inside the
`expect_text` filter_map drops if a row has a non-Text column,
though that path is currently unreachable given the schema.

---

## #30 — Template `enable_log` table semantic muddle

**Where:** `gadgets/template/migrations/001_init.sql` and
`gadgets/template/src/lib.rs` (`enable()` and the
`heartbeat` task).

**The thing:** the `enable_log` table is currently inserted
into from two places — the lifecycle `enable()` and the
scheduled `heartbeat` task. There is no column distinguishing
the two event types. Gadget authors reading the demo will see
a table called `enable_log` containing both kinds of events
and be confused.

**Fix:** add a `reason TEXT NOT NULL DEFAULT 'enable'` column
to the migration. The `enable()` insert keeps the default
(`'enable'`); the `heartbeat` task explicitly inserts with
`reason = 'heartbeat'`. Frontend can then count each type via
`SELECT COUNT(*) FROM enable_log WHERE reason = ?`.

This is a small but pedagogically valuable improvement — it
shows gadget authors the right pattern for multi-event log
tables instead of teaching them to mix unrelated events into
a single undifferentiated table.

**Migration concern:** since the template gadget is a
greenfield example (no production users with existing
databases), this can be a destructive edit to `001_init.sql`
rather than a `002_*.sql` add-column migration. Real gadget
authors should add a new migration file for production
schemas — call this out in a comment.

---

## #31 — Friendlier rejection of Quartz `@daily` / `@hourly` macros

**Where:** `src-tauri/src/wasm/manifest.rs parse_cron_schedule`
(after the existing 5-field token-count check landed in 4eae7b4).

**The thing:** gadget authors who try to write `@daily` or
`@hourly` (Quartz aliases for common schedules) get a generic
"expected 5-field POSIX cron, got 1 field(s)" error. The 1-
field count is technically correct but not actionable — the
author thinks `@daily` is a field, not realizing it's an
alias they should expand.

**Fix:** detect single-token inputs that start with `@` and
emit a specific error suggesting the POSIX equivalent:

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
            "Quartz `@{macro_name}` aliases are not supported — use the equivalent 5-field POSIX expression `{equiv}`"
        );
    }
}
```

Add a couple of new tests (`@daily_rejected_with_hint`,
`@hourly_rejected_with_hint`) covering the alias path.

The existing `quartz_macro_at_daily_rejected` test already
verifies that `@daily` fails — this enhancement just makes
the failure message more useful.

---

## #32 — Calculator `save_history_method` typed payload

**Where:** `gadgets/calculator/src/lib.rs save_history_method`.

**Current pattern:** three chained
`value.get("...").and_then(...).ok_or("missing '...'")?`
calls to extract `expression`, `result`, `resultType`.

**The thing:** the existing form is correct but verbose, and
the error messages are static literals with no context about
what was actually received. A `#[derive(serde::Deserialize)]`
struct would be cleaner and produce better errors.

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

Requires adding `serde` as a direct dep on the calculator
crate (currently only `serde_json` is listed; the derive
macro needs `serde`).

The Rust gadget SDK (`torchsnap-gadget-sdk`) now exposes
`messaging::parse_payload<T: DeserializeOwned>` /
`messaging::to_response<T: Serialize>` for exactly this
pattern — the calculator's `save_history_method` is already
migrated over. If any remaining handler still does manual
parsing, apply the SDK helpers rather than open-coding
another `serde_json::from_str`.

---

## Out-of-scope items still tracked elsewhere

These came up during the review but already have dedicated
todos or were explicitly deferred by the migration plan:

- **Rust gadget SDK crate** — implemented as
  `torchsnap-gadget-sdk` (see `gadgets/gadget-sdk/`).
  Gadgets consume it via `use torchsnap_gadget_sdk::prelude::*;`
  and the `define_gadget!` macro replaces the hand-rolled
  `wit_bindgen::generate!` + `export!` boilerplate.
- **Dedicated `gadgets/test-fixture/` crate** for end-to-end
  WASM bridge integration tests against the wasmtime linker.
  Mentioned by the migration plan as required for D1-D4
  layered tests but never created. Would unblock the SQL
  storage tests that the calculator port had to skip
  (`history_save_and_query`,
  `history_dedup_bumps_timestamp`, `history_filter`).
- **`spawn_blocking` for the scheduler's `run_task` call** —
  the existing
  `01kn89g33581za71v2jbbhx4ah-spawn-blocking-for-tokio-threads.md`
  todo covers this; the scheduler's inline comment now
  documents the threshold (`scheduler_loop` in
  `src-tauri/src/wasm/bridge.rs`).
- **`messaging-stream` WIT sub-interface** for WASM gadget
  streaming RPC. Out of scope per ADR 0030; would also need
  the `01kn30th27x0arv9a5cekkwgpw` channel-lifecycle todo to
  land first.

---

## Suggested PR slicing when this lands

These items are independent and small. Cluster them by area
to keep diffs reviewable:

1. **Frontend polish PR** — #22, #23, #26
2. **Rust polish PR** — #28, #29
3. **Docs polish PR** — #27
4. **Template DX PR** — #30, #31 (the friendlier cron error
   for `@daily` macros has gadget-author user impact, so
   it's worth combining with the template `reason` column
   change)
5. **Test scaffolding PR** — #24 + #25 (the workspace
   tsconfig alias is a prerequisite for the testing entry
   points, and the context tests are the first real users
   of `MockGadgetContextProvider`)
6. **Defer until the Rust SDK crate lands** — #32

No single PR is large enough to justify slicing further. The
test scaffolding one is the biggest because it requires
setting up Vitest + React Testing Library from scratch on the
host frontend.
