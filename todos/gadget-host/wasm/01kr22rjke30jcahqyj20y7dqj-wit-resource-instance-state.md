# Replace `thread_local!` gadget state with WIT resource exports

**Status: needs discussion** — initial idea, not yet validated
against WIT/wasmtime resource export constraints.

## Problem

Every non-trivial WASM gadget uses `thread_local!` with
`RefCell`/`Cell`/`OnceCell` to hold per-instance state across
WIT export calls. The current WIT world exports free functions
(`enable()`, `search()`, `execute()`, etc.), so there is no
`self` parameter — gadget authors are forced to simulate
instance state with module-scoped mutable statics.

Current `thread_local!` usage by category:

| Gadget       | Cell                                     | Purpose                            |
|-------------|------------------------------------------|------------------------------------|
| bangs       | `PENDING_URL: RefCell<Option<String>>`   | search→execute data handoff        |
| open-url    | `PENDING_URL: RefCell<Option<String>>`   | search→execute data handoff        |
| calculator  | `HEURISTIC_ENABLED: Cell<bool>`, etc.    | settings cache                     |
| emoji-picker| `ENTRIES: OnceCell<Vec<EmojiData>>`      | lazy one-time init                 |
| zerotier    | `RUNTIME: RefCell<Runtime>`              | full runtime state struct          |
| hello-world | `PETNAMES: RefCell<Vec<String>>`         | per-instance state                 |

The safety argument is sound (WASM is single-threaded, host
serializes calls on the store mutex), but the ergonomics are
poor:

- Boilerplate: every gadget needs `thread_local!` + `.with()`
  + `borrow()`/`borrow_mut()` ceremony.
- Error-prone: forgetting to seed a cell at `enable()` time
  causes silent `None`/default bugs.
- Unclear ownership: nothing in the type system connects
  `enable()` (which initializes state) to `search()` (which
  reads it) — the connection is implicit.
- Fragile: the `PENDING_URL` pattern breaks silently if the
  host ever pipelines calls or a background task touches the
  cell (see the bangs gadget's safety comment).

Note: the `PENDING_URL` handoff specifically is better solved
by the `data` field on `scored-entry`, already landed as
`ScoredEntry::data`. This todo addresses the broader
instance-state problem that remains after that fix.

## Idea: export a WIT resource instead of free functions

Instead of exporting bare interfaces with free functions, the
gadget world would export a single `resource gadget-instance`
whose constructor replaces `enable()` and whose methods replace
the current free-function exports.

### Current WIT (free functions)

```wit
world gadget {
  import logging;
  import sql;
  // ...

  export lifecycle;    // enable(), disable(), on-setting-changed()
  export search;       // entries(), search(), execute()
  export messaging;    // handle-message()
  export tasks;        // run-task()
}
```

### Proposed WIT (resource export)

```wit
interface gadget-instance {
  use types.{entry-icon, action-id, action};

  // All the same types as today (catalog-entry,
  // scored-entry, search-response, etc.) live here or
  // are `use`d from `types`.

  resource instance {
    /// Constructor replaces `enable()`. The gadget
    /// initializes its state and returns the handle.
    constructor();

    /// Replaces `disable()`. Called before the host
    /// drops the resource handle.
    teardown: func();

    on-setting-changed: func(key: string, value: string);

    entries: func() -> list<catalog-entry>;
    search: func(query: string, matched-prefix: option<string>) -> search-response;
    execute: func(entry-id: string, action-id: action-id) -> result<post-action, string>;

    handle-message: func(method: string, payload: string) -> result<string, string>;
    run-task: func(task-id: string) -> result<_, string>;
  }
}

world gadget {
  import logging;
  import sql;
  // ...

  export gadget-instance;
}
```

### Guest side (Rust)

Today a gadget author writes:

```rust
thread_local! {
    static RUNTIME: RefCell<Runtime> = RefCell::new(Runtime::default());
}

struct MyPlugin;
define_gadget!(MyPlugin);

impl LifecycleGuest for MyPlugin {
    fn enable() {
        RUNTIME.with(|r| {
            *r.borrow_mut() = Runtime::new(/* ... */);
        });
    }
    // ...
}

impl SearchGuest for MyPlugin {
    fn search(query: String, prefix: Option<String>) -> SearchResponse {
        RUNTIME.with(|r| {
            let rt = r.borrow();
            rt.do_search(&query)
        })
    }
    // ...
}
```

With the resource model:

```rust
struct MyPlugin {
    runtime: Runtime,
}

impl GadgetInstanceGuest for MyPlugin {
    fn new() -> Self {
        Self {
            runtime: Runtime::new(/* ... */),
        }
    }

    fn teardown(&self) { /* cleanup */ }

    fn search(&self, query: String, prefix: Option<String>) -> SearchResponse {
        self.runtime.do_search(&query)
    }
    // ...
}

define_gadget!(MyPlugin);
```

State is a plain struct field. No `thread_local!`, no
`RefCell`, no `.with()` ceremony. The `define_gadget!` macro
generates the WIT resource export shims.

### Host side

The host currently calls free-function exports on the
`bindings::Gadget` type. With the resource model:

1. `enable()` → call the resource constructor, receive an
   opaque `ResourceAny` handle, store it in `GadgetState`.
2. All subsequent calls → invoke methods on the stored handle.
3. `disable()` → call `teardown()`, then drop the handle.

wasmtime's `ResourceAny` holds the guest-side index; the
actual struct lives in guest linear memory. The host never
inspects it.

## Open questions

- **wasmtime resource export maturity**: Does wasmtime's
  component model implementation fully support exported
  resources with methods? Need to verify against the wasmtime
  version pinned in `Cargo.lock`.

- **`wit_bindgen` support**: Does `wit_bindgen::generate!`
  produce usable trait signatures for exported resources on
  the guest side? The `pub_export_macro` and
  `default_bindings_module` options would need to work with
  resource exports.

- **Mutable self**: WIT resource methods receive `&self`, but
  gadget state is almost always mutated. The guest would
  still need interior mutability (`RefCell` fields inside the
  struct) — but this is a local concern inside the struct,
  not a module-scoped `thread_local!`. Need to check if
  `wit_bindgen` generates `&self` or `&mut self` for
  resource methods.

- **Migration path**: Every gadget and the SDK's
  `define_gadget!` macro change. The WIT world version bumps
  (0.1.0 → 0.2.0). All existing `.torchsnap` archives stop
  working. Is a compatibility shim worth it, or is a clean
  break acceptable at this stage?

- **Multiple resources vs single**: One monolithic resource
  (shown above) vs separate resources per concern
  (`search-instance`, `messaging-instance`). The monolithic
  approach is simpler for gadget authors; separate resources
  offer finer-grained opt-in but add complexity.

- **Constructor parameters**: Should the constructor receive
  arguments (gadget id, initial settings snapshot) or should
  the gadget call host imports to fetch those during
  construction, as it does today during `enable()`?

## Related

- `ScoredEntry::data` in `src-tauri/src/commands/types.rs`: the
  narrower search to execute data handoff fix, already landed
  independently.
- `gadgets/gadget-sdk/wit/torchsnap-gadget.wit` — current
  WIT world definition.
- `gadgets/gadget-sdk/src/lib.rs` — `define_gadget!` macro.
- `src-tauri/src/wasm/runtime/instance.rs` — host-side
  instance management.
