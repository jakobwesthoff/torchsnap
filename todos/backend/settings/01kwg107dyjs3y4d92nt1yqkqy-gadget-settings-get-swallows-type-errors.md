---
kind: improvement
severity: low
status: open
area: [src-tauri/src/settings/mod.rs]
tags: [error-handling]
---

# GadgetSettings::get conflates missing key and corrupt value

## Problem
`GadgetSettings::get` maps a deserialization failure to `None`
(`src-tauri/src/settings/mod.rs:161-166`):

```rust
pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
    let full_key = format!("{}{}", self.prefix, key);
    self.store
        .get(&full_key)
        .and_then(|v| serde_json::from_value(v).ok())
}
```

A key that exists but holds the wrong JSON type (frontend wrote a
string where the gadget expects a number, or a hand-edited store
file) is indistinguishable from a key that does not exist. The
gadget silently falls back to whatever it does for `None` —
typically its default — and the corrupt value stays in the store
unnoticed. Nothing is logged.

Note the philosophical split with the other settings read path:
`SettingsWatch::get` (`settings/notifier.rs:49-52`) *panics* on
exactly this condition. One path hides the corruption, the other
crashes on it; see
`todos/backend/settings/01kwg107dyjs3y4d92nt1yqkqv-settings-watch-panics-on-bad-value.md`.

## Impact
Gadgets silently run with default settings after a bad write,
with no signal to the user or the logs; debugging "my setting is
ignored" requires inspecting the store file by hand.

## Suggested fix
At minimum, log a warning when the key exists but fails to
deserialize (the `and_then(.ok())` currently erases that
distinction). If the settings failure philosophy gets unified per
the sibling todo, apply the same policy here.
