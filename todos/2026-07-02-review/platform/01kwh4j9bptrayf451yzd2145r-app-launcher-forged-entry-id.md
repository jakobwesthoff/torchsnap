# app-launcher `execute` opens a webview-supplied `entry.id`; forgery is blocked by an incidental entry-store gate, leaving a low-severity cross-gadget app-launch proxy

**Kind:** possible-bug (security)
**Severity:** low (contingent on an incidental control — see below)
**Area:** src-tauri/src/gadgets/app_launcher.rs, src-tauri/src/gadgets/system_preferences.rs, src-tauri/src/gadget_host.rs

## Problem
`AppLauncherGadget::execute` (`app_launcher.rs:231-257`) passes the
`ScoredEntry`'s `id` to the opener without re-checking it against the
discovered-apps cache:

```rust
ActionId::Open   => opener.open_path(&entry.id)...,
ActionId::Reveal => opener.reveal_path(&entry.id)...,
```

For app entries `entry.id` is the app's filesystem path.
`SystemPreferencesGadget::execute` (`system_preferences.rs:151-179`)
is the same shape: `open_url(pane_url(&entry.id))` builds
`x-apple.systempreferences:<entry.id>` from the id. `open_path` routes
to the OS default-open handler and can launch apps
(`opener.rs:110-127`).

The `search_execute` IPC command lets the caller supply the target
`source` and `entry_id` (`commands/mod.rs:47-62`), which raised the
concern that a gadget-controlled webview could forge
`search_execute("app-launcher", "/arbitrary/path", Open)` to open an
arbitrary path.

### Correction — the premise of "arbitrary-path forgery" is wrong
`search_execute` does **not** hand the raw `entry_id` to the gadget.
`GadgetHost::execute` (`gadget_host.rs:979-985`) looks the entry up in
an `EntryStore` and passes the **stored** `ScoredEntry`:

```rust
let Some(entry) = self.entry_store.get(source, entry_id) else {
    eprintln!("execute: entry '{entry_id}' ... not found in entry store \
               (bug — the UI should only execute entries from the current search)");
    return Ok(PostAction::Nothing);
};
```

The `EntryStore` (`entry_store.rs:49-65`) is keyed by
`(source, entry.id)`, populated only from real search results
(`store_sourced_entries` → `insert(entry.source, entry.inner)`,
`gadget_host.rs:832-835`, with `entry.source` set by the host to
`gadget.id()`), and cleared at the top of every `search()`
(`gadget_host.rs:661`). So for `source="app-launcher"` the only
executable `entry_id`s are real discovered `.app` paths;
`/arbitrary/path` → `get()` returns `None` → `PostAction::Nothing`,
nothing opens. Cross-source injection is impossible too: a gadget's
own results land under its own source key, never under
`app-launcher`.

**This gate is load-bearing for security but incidental.** It is
justified in code as a UI-correctness invariant ("bug — the UI should
only execute entries from the current search"), not as a deliberate
security boundary, so a future refactor could remove it without
anyone realizing it was preventing arbitrary `open_path`.

## Reachability (confirmed)
Gadget frontend JS can reach `search_execute` for any `source`:
- Windows `main`/`settings`/`devtools` (`lib.rs:268-300,958`) are all
  covered by the single capability `capabilities/default.json`.
- `search`, `search_execute`, `gadget_message` are application
  commands (`lib.rs:591-614`); they are not listed in the capability
  `permissions` array yet are used and work, so in this app the
  app-defined commands are not per-window ACL-gated (the capability
  list gates only core/plugin commands like `opener:*`, `store:*`).
- `tauri.conf.json` sets `withGlobalTauri: true` (so
  `window.__TAURI__.core.invoke` is available to all JS) and
  `csp: null`.
- Gadget bundles are `import()`ed directly into the launcher/settings
  webviews (`src/gadgets/wasmPluginLoader.ts:56-96`) and render inside
  the launcher React tree (`src/launcher/Launcher.tsx:226-236`). There
  is no separate window/webview or narrower capability for gadget UI.

Gadget frontend JS runs once the gadget's view is rendered at least
once (bundles lazily `import()` on mount); an inline view renders as
soon as the gadget returns a result for a typed query, after which its
JS can drive IPC while the launcher is open.

## Impact
Because of the entry-store gate, the realistic residual is not
arbitrary open. A malicious gadget frontend can:
1. `invoke("search", { query: "<app name>" })` to make app-launcher
   populate the entry_store with a real installed-app entry
   (`search_catalogs_static` stores any scored title/keyword match),
   then
2. `invoke("search_execute", { source: "app-launcher",
   entryId: "<that app's real path>", actionId: "Open" })`.

This silently launches any **installed** application, with no user
interaction at the moment of launch and regardless of the malicious
gadget's own capability grants (it need hold no opener cap) — a
cross-gadget privilege proxy through app-launcher's
`schemes:["*"], open_path:true` grant (`app_launcher.rs:58-64`).

Exploitability ceiling:
- `open_path` gets only a path (no arguments), constrained to
  discovered `.app` bundles: not arbitrary path, not argument
  injection, not an attacker-planted file, so **not arbitrary code
  execution**. Impact is unwanted app launches (annoyance / mild DoS
  by launching many) plus whatever a given installed app does on plain
  launch.
- Gadget assets are served in-memory from the archive, explicitly not
  extracted to disk (`wasm/protocol.rs:8-11`), so there is no on-disk
  gadget-controlled executable to aim `open_path` at by default. ACE
  would require chaining a separate write primitive (a `command`
  grant, or a filesystem-write grant landing where app-launcher
  discovers it).
- system-preferences is lower: `pane_url` always prefixes
  `x-apple.systempreferences:` (`settings_discovery.rs:34`), the id is
  entry_store-gated to discovered pane ids, and its grant is
  `open_url`-only (`open_path:false, reveal_path:false`). Ceiling:
  open a real System Settings pane.

Severity **low** — a consent/annoyance issue plus a capability-model
leak (a gadget bypasses its own opener grant by proxying through
app-launcher). No memory-safety or code-exec. **Flag:** the low rating
rests on the incidental entry_store gate; if that gate is
removed/refactored the severity reverts to arbitrary `open_path`
(high). Treat the gate as explicitly security-relevant.

## Suggested fix
**(a) Gadget-level cache re-validation (defense-in-depth; cheap).** In
`AppLauncherGadget::execute`, before `open_path`/`reveal_path`, verify
`entry.id` is present in `self.cache` (the discovered
`Vec<DiscoveredApp>`) and reject otherwise; in
`SystemPreferencesGadget::execute`, verify `entry.id` is a discovered
pane before building the URL. This makes each gadget self-defending
independent of the entry_store gate and pins the security-relevance of
the check in the gadget that owns the risk. Be clear what it does
*not* do: for the current threat it is redundant with the entry_store
gate (both restrict to the same discovered set), so it does not close
the "launch any installed app" proxy — app-launcher's legitimate
entries *are* the installed apps.

**(b) Root cause — cross-gadget execute authorization (the durable
fix; architectural, raise with Jakob before implementing).** The
"launch any installed app / open any pane" proxy is not closable by
cache validation. The root cause is that gadget frontend JS runs at
host origin in the same webview as the trusted launcher UI, inheriting
the full host IPC surface: `search_execute` for any `source`,
`gadget_message` for any `source` (`gadget_host.rs:1010`), and every
other app command. `search_execute` is inherently cross-gadget by
design (the unified launcher dispatches the user's selection across
gadgets), so cross-source calls cannot simply be forbidden at the
command without breaking the launcher. Options to weigh:
- Isolate gadget custom-UI in a separate webview/window with a
  narrower capability set and a per-gadget IPC surface, so gadget JS
  cannot invoke `search`/`search_execute`/`gadget_message` for other
  sources. Durable fix, largest change.
- A per-invocation authorization scheme distinguishing
  host-UI-originated `execute` (user selected a result) from
  gadget-originated calls. Hard to enforce within one shared-origin
  webview, which is why webview isolation is the cleaner path.

The same shared-origin exposure applies to `gadget_message(source, …)`
for arbitrary source and to every app command, so it warrants a
broader pass than B18 alone.

## Forward-looking risk (record explicitly)
If any gadget ever emits *arbitrary filesystem paths* as entry ids
with an `open_path` Open action, the same cross-source proxy would let
a malicious gadget open those — and the entry_store gate would **not**
help, because those paths would legitimately live in that gadget's
store. That scenario would be materially higher severity than the
current app-launcher case. Any new gadget with an open-path action
over gadget-chosen paths must be reviewed against this.

## Key files
`gadget_host.rs` (`execute` :952-1008, `search`/store :660-836),
`entry_store.rs`, `gadgets/app_launcher.rs`,
`gadgets/system_preferences.rs`, `caps/opener.rs`, `wasm/protocol.rs`,
`src/gadgets/wasmPluginLoader.ts`, `capabilities/default.json`,
`tauri.conf.json`.
