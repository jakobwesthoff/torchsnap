---
title: "Replace `thread_local!` gadget state with WIT resource exports"
kind: improvement
component: gadget-sdk
status: needs-discussion
tags: [api-design]
---
# Replace `thread_local!` gadget state with WIT resource exports

Gadget authors keep per-instance state in `thread_local!` cells, because the WIT world exports free functions and so there is no `self` to hold state. This todo proposes exporting a WIT resource instead. It is an initial idea, not yet validated against WIT and wasmtime resource export constraints.

## Problem

The safety argument for `thread_local!` is sound (WASM is single-threaded, and the host serializes calls on the store mutex), but the ergonomics are poor:

- Boilerplate: every gadget needs `thread_local!` plus `.with()` and `borrow()` / `borrow_mut()` ceremony.
- Error-prone: forgetting to seed a cell at `enable()` time causes silent `None` or default bugs.
- Unclear ownership: nothing in the type system connects `enable()` (which initializes state) to `search()` (which reads it). The connection is implicit.
- Fragile: the `PENDING_URL` pattern breaks silently if the host ever pipelines calls or a background task touches the cell (see the bangs gadget's safety comment).

Current `thread_local!` usage by category:

| Gadget       | Cell                                     | Purpose                            |
|-------------|------------------------------------------|------------------------------------|
| calculator  | `HEURISTIC_ENABLED: Cell<bool>`, etc.    | settings cache                     |
| emoji-picker| `ENTRIES: OnceCell<Vec<EmojiData>>`      | lazy one-time init                 |
| zerotier    | `RUNTIME: RefCell<Runtime>`              | full runtime state struct          |
| hello-world | `PETNAMES: RefCell<Vec<String>>`         | per-instance state                 |
| awake       | `RUNTIME: RefCell<...>`                  | per-instance state                 |

The `PENDING_URL` handoff that `bangs` and `open-url` used to need is gone. ADR 0055 and ADR 0056 gave every entry action a gadget-defined `command: string`, and `execute(command)` now receives that command directly. Both gadgets moved the URL into a typed `Command` enum (see `gadgets/bangs/src/lib.rs`, `enum Command` and `execute`) instead of stashing it in `thread_local!` state. This todo addresses the broader instance-state problem that remains for the gadgets listed above.

## Idea: export a WIT resource instead of free functions

Instead of exporting bare interfaces with free functions, the gadget world would export a single `resource gadget-instance` whose constructor replaces `enable()` and whose methods replace the current free-function exports.

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
  use types.{entry-icon, entry-actions};

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
    execute: func(command: string) -> result<post-action, string>;

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

State is a plain struct field. There is no `thread_local!`, no `RefCell` and no `.with()` ceremony. The `define_gadget!` macro generates the WIT resource export shims.

### Host side

The host currently calls free-function exports on the `bindings::Gadget` type. With the resource model:

1. `enable()` calls the resource constructor and receives an opaque `ResourceAny` handle, which it stores in `GadgetState`.
2. All subsequent calls invoke methods on the stored handle.
3. `disable()` calls `teardown()`, then drops the handle.

wasmtime's `ResourceAny` holds the guest-side index. The actual struct lives in guest linear memory, and the host never inspects it.

## Open questions

- **wasmtime resource export maturity**: does wasmtime's component model implementation fully support exported resources with methods? Verify against the wasmtime version pinned in `Cargo.lock`.
- **`wit_bindgen` support**: does `wit_bindgen::generate!` produce usable trait signatures for exported resources on the guest side? The `pub_export_macro` and `default_bindings_module` options would need to work with resource exports.
- **Mutable self**: WIT resource methods receive `&self`, but gadget state is almost always mutated. The guest would still need interior mutability (`RefCell` fields inside the struct). That is a local concern inside the struct, not a module-scoped `thread_local!`. Check whether `wit_bindgen` generates `&self` or `&mut self` for resource methods.
- **Migration path**: every gadget and the SDK's `define_gadget!` macro change, and the WIT world version bumps (0.1.0 to 0.2.0). All existing `.torchsnap` archives stop working. Is a compatibility shim worth it, or is a clean break acceptable at this stage?
- **Multiple resources or one**: one monolithic resource (shown above) or separate resources per concern (`search-instance`, `messaging-instance`)? The monolithic approach is simpler for gadget authors. Separate resources offer finer-grained opt-in but add complexity.
- **Constructor parameters**: should the constructor receive arguments (gadget ID, initial settings snapshot), or should the gadget call host imports to fetch those during construction, as it does today during `enable()`?

## Related

- ADR 0055 and ADR 0056: fixed action slots and gadget commands, the fix that already retired the `PENDING_URL`-style search-to-execute handoff. See `docs/adr/0055-dispatch-entry-actions-through-gadget-commands-in-fixed-slots.md` and `docs/adr/0056-version-the-wit-contract-and-both-gadget-sdks-in-lockstep.md`.
- `gadgets/gadget-sdk/wit/torchsnap-gadget.wit`: the current WIT world definition.
- `gadgets/gadget-sdk/src/lib.rs`: the `define_gadget!` macro.
- `src-tauri/src/wasm/runtime/instance.rs`: host-side instance management.
