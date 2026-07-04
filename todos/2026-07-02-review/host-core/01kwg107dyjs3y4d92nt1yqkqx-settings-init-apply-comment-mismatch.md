# SettingsInit::apply rewrites every key while its comment claims it filters

**Kind:** refactor
**Severity:** low
**Area:** src-tauri/src/settings/mod.rs

## Problem
`SettingsInit::apply` writes back *every* entry it holds — all
keys loaded by `from_store` plus any defaults added by `ensure`
(`src-tauri/src/settings/mod.rs:103-116`):

```rust
pub fn apply<R: tauri::Runtime>(self, store: &Arc<Store<R>>, prefix: &str) {
    for (key, value) in self.entries {
        let full_key = format!("{prefix}{key}");
        // Only write keys that don't already exist or whose value
        // changed. Since `from_store` loaded the current state and
        // `ensure` only fills gaps, most keys will already match —
        // but migrations via `remove` + `ensure` can produce new
        // values that need writing.
        store.set(full_key, value);
    }
    let _ = store.save();
}
```

The inline comment describes a filter ("Only write keys that
don't already exist or whose value changed") that the code does
not implement: `store.set` runs unconditionally for every entry.
Two secondary points in the same function:

- `tauri_plugin_store::Store::set` fires change listeners per
  call, so every startup replays a change notification for every
  existing key of every gadget namespace that goes through a
  `SettingsInit` cycle — any frontend `onChange`/`onKeyChange`
  subscriber sees a burst of no-op "changes" at startup.
- `let _ = store.save()` discards the save error. If persisting
  fails (disk full, permissions), freshly ensured defaults exist
  only in memory; the frontend store instance loading the same
  file (which is exactly what the comment above the `save()` says
  the save is for) reads a file without them, and nothing is
  logged.

## Impact
No data corruption: values written equal values read. The cost is
misleading documentation (the next reader assumes diffing exists
and may rely on it), a startup burst of spurious store-change
events, and silent loss of defaults on save failure.

## Suggested fix
Make code and comment agree — the cheapest correct version is to
actually diff: track which keys `ensure`/`remove` touched and
write only those. That also eliminates the spurious change-event
burst. Log the `store.save()` error instead of discarding it.
