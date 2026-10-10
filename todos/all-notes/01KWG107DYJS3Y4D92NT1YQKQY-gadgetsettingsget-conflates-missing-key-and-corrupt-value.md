---
title: "GadgetSettings::get conflates missing key and corrupt value"
kind: improvement
component: backend
impact: low
tags: [error-handling]
---
# GadgetSettings::get conflates missing key and corrupt value

`GadgetSettings::get` in `src-tauri/src/settings/mod.rs` returns `None` both when a key is missing and when it holds a value of the wrong JSON type. A bad write therefore makes a gadget silently fall back to its default, and nothing is logged.

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
gadget silently falls back to whatever it does for `None`,
typically its default, and the corrupt value stays in the store
unnoticed. Nothing is logged.

Note the philosophical split with the other settings read path:
`SettingsWatch::get` (`src-tauri/src/settings/notifier.rs:49-52`) *panics* on
exactly this condition. One path hides the corruption, the other
crashes on it; see the SettingsWatch::get todo under Relations.

## Impact
Gadgets silently run with default settings after a bad write,
with no signal to the user or the logs; debugging "my setting is
ignored" requires inspecting the store file by hand.

## Suggested fix
At minimum, log a warning when the key exists but fails to
deserialize (the `and_then(.ok())` currently erases that
distinction). If the settings failure philosophy gets unified per
the SettingsWatch::get todo, apply the same policy here.

## Relations

- Relates to: [SettingsWatch::get panics on type-mismatched or deleted setting values](01KWG107DYJS3Y4D92NT1YQKQV-settingswatchget-panics-on-type-mismatched-or-deleted-setting-values.md), the opposite failure policy on the same condition.
