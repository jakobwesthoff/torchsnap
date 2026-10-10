---
title: "Use the helper idiom in the settings reactivity architecture doc example"
kind: docs
component: gadget-host
impact: low
origin: review
---
# Use the helper idiom in the settings reactivity architecture doc example

The architecture doc's example for reading a WASM setting uses a raw `unwrap_or_else` form that real gadgets do not use. Found in the post-migration validation review of 2026-04-08 (seven parallel review agents covering D1-D6 and the calculator port) as one of the lower-priority polish items deliberately deferred. It does not block anything. The item keeps its original review section number, #27, so back-references stay intact.

## Scope

**Where:** `docs/Gadget-Architecture/06-settings-reactivity.md`, lines around 109 to 111.

**The thing:** the inline example reads a WASM setting as:

```rust
serde_json::from_str(
    &settings::get("verbose").unwrap_or_else(|| "false".into())
)
```

It is verbose and slightly misleading. The template gadget and the calculator both use small `read_bool_setting` / `read_string_setting` helpers. New gadget authors reading the architecture doc will copy the verbose form.

**Fix:** either replace the example with the helper form (and refer to the Rust SDK crate), or add a one-line note under the example pointing at the helper idiom in `gadgets/template/src/lib.rs`. The helper form is cleaner and reflects what real gadgets do.

The Rust gadget SDK crate exists as `torchsnap-gadget-sdk` (see `gadgets/gadget-sdk/`). Gadgets consume it through `use torchsnap_gadget_sdk::prelude::*;`, and the `define_gadget!` macro replaces the hand-rolled `wit_bindgen::generate!` and `export!` boilerplate.

## Done when

- The example in `docs/Gadget-Architecture/06-settings-reactivity.md` no longer shows the raw `unwrap_or_else` form, or carries a note pointing at the helper idiom.
