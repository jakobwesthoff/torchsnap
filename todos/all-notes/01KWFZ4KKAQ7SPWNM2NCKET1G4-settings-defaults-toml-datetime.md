---
kind: question
severity: low
status: open
area: [src-tauri/src/wasm/manifest/mod.rs]
---

# [settings] defaults accept TOML types with no JSON equivalent

## Problem
Manifest `[settings]` defaults are stored as raw
`HashMap<String, toml::Value>` and documented as "arbitrary
JSON-compatible types"
(`src-tauri/src/wasm/manifest/mod.rs:61-66`). TOML has a
first-class datetime type that JSON lacks, and nothing at parse
time rejects it:

```toml
[settings]
last-reset = 2026-01-01T00:00:00Z
```

parses fine into `toml::Value::Datetime`. The consuming code is
`WasmGadgetBridge::initialize_settings`
(`src-tauri/src/wasm/bridge.rs:563-570`):

```rust
if let Ok(json_value) = serde_json::to_value(toml_value) {
    settings = settings.ensure(key, json_value);
}
```

so a default whose TOML→JSON conversion fails is *silently
dropped* (the `if let Ok` swallows the error, no log), and one
that converts to an unexpected JSON shape is applied as-is.
Either way the gadget author gets no diagnostic.

## Impact
Unclear failure mode for a manifest that TOML considers valid.
Low likelihood (authors rarely put datetimes in defaults), but
the behavior is currently unspecified and untested.

## Suggested fix
Check what the defaults-application code does with
`toml::Value::Datetime` (and non-string table keys, if
applicable). Either reject datetimes at `Manifest::parse` time
with a clear error, or define and test the conversion. A parse
test with a datetime default should pin whichever behavior is
chosen.
