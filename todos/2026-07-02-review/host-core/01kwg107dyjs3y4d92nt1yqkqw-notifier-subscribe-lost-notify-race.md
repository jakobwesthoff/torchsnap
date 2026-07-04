# SettingsNotifier: notify() racing a first subscribe is silently dropped

**Kind:** possible-bug
**Severity:** low
**Area:** src-tauri/src/settings/notifier.rs

## Problem
`SettingsNotifier::notify` ignores keys that have no channel yet
(`src-tauri/src/settings/notifier.rs:140-149`):

```rust
pub fn notify(&self, key: &str, value: Value) {
    let channels = self.channels.lock().expect("channels not poisoned");
    if let Some(tx) = channels.get(key) {
        let _ = tx.send(value);
    }
}
```

Subscribers create the channel via `watch_with_initial(key,
initial)` where `initial` is a value the *caller* read from the
store beforehand (`notifier.rs:116-135`). Between that store read
and the channel insertion under the lock, a concurrent
`settings-changed` event can fire `notify()` — which finds no
channel and drops the update. The subscriber then starts with the
stale pre-change value and, because `watch` only wakes on the
*next* send, stays stale until the setting changes again.

The same shape exists for two subscribers: the second caller's
`initial` is documented as ignored when the channel exists — fine
— but the first-subscription race window is real and invisible.

## Impact
A settings change performed exactly during subsystem startup
(frontend restores UI state and writes settings while the backend
is still wiring up FrecencyStore / control socket / metadata
service) is lost. The affected subsystem runs with the old value
indefinitely. Low probability, but the failure is permanent until
the next change and leaves no trace.

Related in spirit to the CoalescingDispatcher lost-wakeup finding
(`host-core/01kwg0b6tyfptjcafw5tmag25w-coalescing-dispatcher-lost-wakeup.md`):
both settings pathways have subscribe/dispatch races at the edges.

## Suggested fix
Have `watch_with_initial` take the store (or a value-provider
closure) and perform the read *inside* the channels lock, so the
initial value and channel creation are atomic with respect to
`notify()`. Alternatively re-read the store immediately after
inserting the channel and `send()` if the value differs from
`initial`.
