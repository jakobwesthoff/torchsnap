---
title: "Stop gadget frontends from launching apps through the app-launcher execute path"
kind: bug
component: gadgets
status: needs-discussion
impact: low
tags: [security]
---

# Stop gadget frontends from launching apps through the app-launcher execute path

A gadget's frontend JS can call `search_execute` for any source, so it
can launch any installed application through app-launcher, even though
the gadget holds no opener capability itself. The entry store blocks
arbitrary paths, which keeps the impact low. That gate is incidental,
though, not a designed security boundary.

The entry-store lookup acts as an incidental control; see below for nuance.

Since ADR 0055/0056 (fixed action slots, typed gadget commands), the
shape changed from an `ActionId` selecting a field on `entry.id` to a
`Command` enum carrying the path as payload, but the same gate and the
same residual risk apply.

## Problem

`AppLauncherGadget::execute` (`app_launcher.rs:236-253`) passes the
path carried by the command straight to the opener, without
re-checking it against the discovered-apps cache:

```rust
match command {
    Command::Open(id) => opener.open_path(&id)...,
    Command::Reveal(id) => opener.reveal_path(&id)...,
}
```

For app entries, `id` is the app's filesystem path, produced by
`app_actions(&app.id)` (`app_launcher.rs:204`) at search time and
stored on the entry's `EntryActions`. `SystemPreferencesGadget::execute`
(`system_preferences.rs:161-178`) has the same shape:
`Command::OpenPane(pane_id)` builds `x-apple.systempreferences:<pane_id>`
via `pane_url`. `open_path` routes to the OS default-open handler and can
launch apps (`opener.rs:110-127`).

The `search_execute` IPC command lets the caller supply the target
`source`, `entry_id` and `slot` (`commands/mod.rs:47-62`). That raised
the concern that a gadget-controlled webview could forge
`search_execute("app-launcher", "/arbitrary/path", Slot::Primary)` to
open an arbitrary path.

### Correction: the premise of "arbitrary-path forgery" is wrong

`search_execute` does **not** hand the caller-supplied path to the
gadget. `GadgetHost::execute` (`gadget_host.rs:1022-1049`) resolves the
command through `resolve_command`, which looks the entry up in an
`EntryStore` and reads the command string stored on the requested slot:

```rust
fn resolve_command(
    store: &EntryStore,
    source: &str,
    entry_id: &str,
    slot: Slot,
) -> Result<String, Unresolved> {
    let entry = store.get(source, entry_id).ok_or(Unresolved::UnknownEntry)?;
    entry.actions.get(slot).map(|action| action.command.clone())
        .ok_or(Unresolved::EmptySlot)
}
```

Both failure cases (`UnknownEntry`, `EmptySlot`) return
`PostAction::Nothing` without dispatching to the gadget
(`gadget_host.rs:1035-1048`).

The `EntryStore` (`entry_store.rs:45-90`) is keyed by
`(source, entry.id)`, populated only from real search results, and
cleared at the top of every `search()`. So for `source="app-launcher"`
the only resolvable commands are the ones the gadget itself produced
from real discovered `.app` paths. A caller-supplied `/arbitrary/path`
never reaches the gadget, because it is not a valid `entry_id`/`slot`
pair in the store. Cross-source injection is impossible too: a gadget's
own results land under its own source key, never under `app-launcher`.

**This gate is load-bearing for security but incidental.** It is
justified in code as a UI-correctness invariant ("bug: the UI should
only execute entries from the current search"), not as a deliberate
security boundary. A future refactor could remove it without anyone
realizing it was preventing arbitrary `open_path`.

## Reachability (confirmed)

Gadget frontend JS can reach `search_execute` for any `source`:

- Windows `main`/`settings`/`devtools` (`lib.rs:268-300,958`) are all
  covered by the single capability `capabilities/default.json`.
- `search`, `search_execute`, `gadget_message` are application
  commands (`lib.rs:591-614`). They are not listed in the capability
  `permissions` array yet are used and work, so in this app the
  app-defined commands are not per-window ACL-gated. The capability
  list gates only core and plugin commands like `opener:*` and `store:*`.
- `tauri.conf.json` sets `withGlobalTauri: true` (so
  `window.__TAURI__.core.invoke` is available to all JS) and
  `csp: null`.
- Gadget bundles are `import()`ed directly into the launcher and
  settings webviews (`src/gadgets/wasmPluginLoader.ts:56-96`) and render
  inside the launcher React tree (`src/launcher/Launcher.tsx:226-236`).
  There is no separate window, webview or narrower capability for gadget
  UI.

Gadget frontend JS runs once the gadget's view is rendered at least
once (bundles lazily `import()` on mount). An inline view renders as
soon as the gadget returns a result for a typed query, after which its
JS can drive IPC while the launcher is open.

## Impact

Because of the entry-store gate, the realistic residual is not arbitrary
open. A malicious gadget frontend can:

1. `invoke("search", { query: "<app name>" })` to make app-launcher
   populate the entry_store with a real installed-app entry
   (`search_catalogs_static` stores any scored title or keyword match),
   then
2. `invoke("search_execute", { source: "app-launcher",
   entryId: "<that app's real path>", slot: "primary" })`.

This silently launches any **installed** application, with no user
interaction at the moment of launch, regardless of the malicious
gadget's own capability grants (it need hold no opener cap). It is a
cross-gadget privilege proxy through app-launcher's
`schemes:["*"], open_path:true` grant (`app_launcher.rs:58-64`).

Exploitability ceiling:

- `open_path` gets only a path (no arguments), constrained to discovered
  `.app` bundles. It is not an arbitrary path, not argument injection,
  and not an attacker-planted file, so it is **not arbitrary code
  execution**. The impact is unwanted app launches (annoyance, mild DoS
  by launching many), plus whatever a given installed app does on plain
  launch.
- Gadget assets are served in-memory from the archive, explicitly not
  extracted to disk (`wasm/protocol.rs:8-11`). There is no on-disk
  gadget-controlled executable to aim `open_path` at by default. ACE
  would require chaining a separate write primitive (a `command` grant,
  or a filesystem-write grant landing where app-launcher discovers it).
- system-preferences is lower. `pane_url` always prefixes
  `x-apple.systempreferences:` (`settings_discovery.rs:34`), the id is
  entry_store-gated to discovered pane ids, and its grant is
  `open_url`-only (`open_path:false, reveal_path:false`). The ceiling is
  opening a real System Settings pane.

Severity **low**: a consent and annoyance issue plus a capability-model
leak (a gadget bypasses its own opener grant by proxying through
app-launcher). There is no memory-safety or code-exec issue. **Flag:**
the low rating rests on the incidental entry_store gate. If that gate is
removed or refactored, the severity reverts to arbitrary `open_path`
(high). Treat the gate as explicitly security-relevant.

## Suggested fix

**(a) Gadget-level cache re-validation (defense-in-depth; cheap).** In
`AppLauncherGadget::execute`, before `open_path`/`reveal_path`, verify
that the path carried by `Command::Open`/`Command::Reveal` is present in
`self.cache` (the discovered `Vec<DiscoveredApp>`), and reject it
otherwise. In `SystemPreferencesGadget::execute`, verify that the pane id
carried by `Command::OpenPane` is a discovered pane before building the
URL. This makes each gadget self-defending, independent of the
entry_store gate, and pins the security relevance of the check in the
gadget that owns the risk. Be clear about what it does *not* do: for the
current threat it is redundant with the entry_store gate, since both
restrict to the same discovered set. It does not close the "launch any
installed app" proxy, because app-launcher's legitimate entries *are* the
installed apps.

**(b) Root cause: cross-gadget execute authorization (the durable fix;
architectural, raise with Jakob before implementing).** The "launch any
installed app or open any pane" proxy cannot be closed by cache
validation. The root cause is that gadget frontend JS runs at host origin
in the same webview as the trusted launcher UI, inheriting the full host
IPC surface: `search_execute` for any `source`, `gadget_message` for any
`source` (`commands/mod.rs:75`), and every other app command.
`search_execute` is inherently cross-gadget by design, because the
unified launcher dispatches the user's selection across gadgets. So
cross-source calls cannot simply be forbidden at the command without
breaking the launcher. Options to weigh:

- Isolate gadget custom UI in a separate webview or window with a
  narrower capability set and a per-gadget IPC surface, so gadget JS
  cannot invoke `search`, `search_execute` or `gadget_message` for other
  sources. This is the durable fix and the largest change.
- A per-invocation authorization scheme that distinguishes
  host-UI-originated `execute` (the user selected a result) from
  gadget-originated calls. This is hard to enforce within one
  shared-origin webview, which is why webview isolation is the cleaner
  path.

The same shared-origin exposure applies to `gadget_message(source, ...)`
for arbitrary sources and to every app command, so it warrants a broader
pass than one review finding.

## Forward-looking risk

If any gadget ever emits *arbitrary filesystem paths* as entry IDs with
an `open_path` Open action, the same cross-source proxy would let a
malicious gadget open those. The entry_store gate would **not** help,
because those paths would legitimately live in that gadget's store. That
scenario would be materially higher severity than the current
app-launcher case. Any new gadget with an open-path action over
gadget-chosen paths must be reviewed against this.

## Key files

`gadget_host.rs` (`execute` :1022-1062, `search` from :724),
`entry_store.rs`, `gadgets/app_launcher.rs`,
`gadgets/system_preferences.rs`, `caps/opener.rs`, `wasm/protocol.rs`,
`src/gadgets/wasmPluginLoader.ts`, `capabilities/default.json`,
`tauri.conf.json`.

## Relations

- Relates to: [open_path and reveal_path lack a root constraint, and the open_url wildcard skips URL parsing](01KWH4J9BPTRAYF451YZD2145N-openpath-and-revealpath-lack-a-root-constraint-and-the-openurl-wildcard.md), the app-launcher proxy reaches `open_path` without an opener grant, gated only by the entry store
