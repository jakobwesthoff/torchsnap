---
title: "[settings] defaults accept TOML types with no JSON equivalent"
kind: bug
component: gadget-host
status: needs-discussion
impact: low
---
# [settings] defaults accept TOML types with no JSON equivalent

Manifest `[settings]` defaults accept TOML datetimes, which have no JSON equivalent. A default whose TOML-to-JSON conversion fails is dropped without a log line, so a manifest that TOML considers valid gives the gadget author no diagnostic. The intended behavior is undecided.

## Problem
Read in the code. No test pins the datetime case.

Manifest `[settings]` defaults are stored as raw `HashMap<String, toml::Value>` and documented as "arbitrary JSON-compatible types" (`src-tauri/src/wasm/manifest/mod.rs`, around lines 61 to 66). TOML has a first-class datetime type that JSON lacks, and nothing at parse time rejects it:

```toml
[settings]
last-reset = 2026-01-01T00:00:00Z
```

That parses fine into `toml::Value::Datetime`. The consuming code is `WasmGadgetBridge::initialize_settings` (`src-tauri/src/wasm/bridge.rs`, around lines 563 to 570):

```rust
if let Ok(json_value) = serde_json::to_value(toml_value) {
    settings = settings.ensure(key, json_value);
}
```

A default whose TOML-to-JSON conversion fails is silently dropped: the `if let Ok` swallows the error and nothing is logged. A default that converts to an unexpected JSON shape is applied as-is. Either way the gadget author gets no diagnostic.

## Impact
Unclear failure mode for a manifest that TOML considers valid. Low likelihood, since authors rarely put datetimes in defaults, but the behavior is currently unspecified and untested.

## Suggested fix
Check what the defaults-application code does with `toml::Value::Datetime` (and non-string table keys, if applicable). Then either reject datetimes at `Manifest::parse` time with a clear error, or define and test the conversion. A parse test with a datetime default should pin whichever behavior is chosen.
