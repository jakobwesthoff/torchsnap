---
kind: bug
severity: medium
status: open
area: [src-tauri/src/settings/notifier.rs]
tags: [unconfirmed]
---

# SettingsWatch::get panics on type-mismatched or deleted setting values

## Problem
`SettingsWatch<T>::get` unconditionally expects the stored JSON
value to deserialize into `T`
(`src-tauri/src/settings/notifier.rs:49-52`):

```rust
pub fn get(&self) -> T {
    let value = self.rx.borrow();
    serde_json::from_value(value.clone()).expect("settings watch value deserializes to T")
}
```

The docstring argues this "should not happen if defaults were
initialized correctly via `SettingsInit`" — but `SettingsInit`
only fills *missing* keys once at startup. The values that flow
into the watch channels afterwards come from the
`settings-changed` listener in `src-tauri/src/lib.rs:889-912`,
which re-reads whatever is currently in the store and explicitly
substitutes `Value::Null` when the key is absent:

```rust
let value = store_for_listener
    .get(&payload.key)
    .unwrap_or(serde_json::Value::Null);
notifier.notify(&payload.key, value.clone());
```

So any of the following puts a non-`T` value into the channel and
makes every subsequent `get()` (and therefore `changed()` /
`blocking_changed()`, which call `get()`) panic:

- the frontend writes a value of the wrong type to the store key
  (nothing on the Rust side validates types on write),
- the frontend (or anything else) *deletes* the key and emits
  `settings-changed` → subscribers receive `Null`,
- a user hand-edits the store JSON file on disk to a wrong type
  and the key later gets notified.

## Impact
Subscribers of `SettingsWatch` are long-lived background
consumers (per the module comment: FrecencyStore, control
socket, WebsiteMetadataService). A panic inside
`changed().await` unwinds the owning tokio task; a panic inside
`blocking_changed()` kills the dedicated background thread (e.g.
a retention-cleanup loop). Either way the subsystem silently
stops reacting to settings — or stops working entirely — based
on a store write the backend never validated.

## Suggested fix
Make `get()` fall back instead of panicking: return
`Option<T>`/`Result<T>` or keep the last good value (e.g. store
the deserialized `T` in the channel instead of raw
`serde_json::Value`, validating at `notify()` time and dropping
invalid updates with a warning log). Validating at the
`lib.rs` listener boundary (one place) is cheaper than trusting
every subscriber. Related: `GadgetSettings::get`
(`settings/mod.rs:161-166`) takes the opposite approach and
silently returns `None` on the same condition — the two read
paths should agree on a failure philosophy.
