# Refactor: Plugin execute() data parameter

## Problem

Currently, plugins that need to pass structured data from `search()` through to
`execute()` must encode it into the `entry_id` string (e.g. as JSON). This is a
workaround — `entry_id` is semantically an identifier, not a data transport
mechanism.

The DuckDuckGo Bangs plugin is the first case where this becomes necessary:
`execute()` needs both the bang trigger and the original query to construct the
final URL.

## Proposed Solution

Add an optional opaque data parameter to the plugin entry/execute flow:

1. When a plugin creates a `ScoredEntry` in `search()`, it can attach an
   optional `data: Option<serde_json::Value>` (or `Option<Vec<u8>>`).
2. The `PluginHost` does **not** serialize this data into the frontend-bound
   `SearchMessage`. Instead, it stores the data in a transient in-memory map
   keyed by a generated short-lived ID (or the entry_id itself).
3. The `entry_id` passed to the frontend remains a lightweight string.
4. When `execute(entry_id, action_id, app)` is called, the host looks up the
   stored data and passes it to the plugin as an additional parameter (or
   wraps it in a context struct).
5. The in-memory store is scoped to the current search session — cleared on
   each new query to avoid unbounded growth.

### Benefits

- `entry_id` stays a clean identifier.
- Arbitrary structured data flows from `search()` to `execute()` without
  serialization hacks.
- Data never touches the frontend, avoiding unnecessary IPC overhead.
- Bounded memory: cleared per search session.

### Considerations

- The `execute()` trait signature changes — breaking change for all plugins.
- Need to decide: `Option<Value>` (flexible) vs typed per-plugin (complex).
- Catalog entries (from `entries()`) don't have this problem since they're
  static — only dynamic `search()` results benefit.
