# Replace Spin-Poll Loop with Shared Channel Fan-In

## Problem

`plugin_host.rs` non-prefix query path uses a `try_recv` / `yield_now` polling
loop to drain results from multiple query plugins. This is a busy-wait that
wastes CPU. It also has an O(N²) cleanup issue: `swap_remove` handles one
disconnected receiver per outer loop iteration.

## Decision

Replace per-plugin channels with a single shared `mpsc::channel`. All plugins
send into the same pipe; the host does a simple `rx.recv().await` loop.

No new dependencies needed — no `tokio-stream`, no `StreamMap`.

## Design

### Producer side

One shared channel. Each plugin gets a clone of the sender, wrapped in a
`ResultChannel` that tags messages with the plugin's source id:

```rust
let (tx, mut rx) = mpsc::channel::<(String, PluginResponse)>(4 * plugin_count);

for plugin in &self.plugins {
    let source = plugin.id().to_string();
    let tx = tx.clone();

    tokio::task::spawn_blocking(move || {
        let rc = ResultChannel::new(source, tx);
        plugin.search(&query, None, &rc, &cancel);
        // tx drops here — when all senders drop, rx closes
    });
}

drop(tx); // drop original so rx closes when all plugins finish
```

### Consumer side

```rust
while let Some((source, response)) = rx.recv().await {
    // ... existing business logic unchanged ...
}
```

`recv().await` parks the task until a message arrives. Returns `None` when all
senders are dropped (all plugins done). No polling, no manual removal tracking.

### ResultChannel changes

`ResultChannel` gains a `source: String` field set at construction. The plugin
receives `&ResultChannel` and cannot access or mutate the source id — it's a
private field on an opaque struct behind a shared reference. This is also
forward-compatible with the WASM plugin model, where the host function bakes in
the source id and the sandbox prevents any tampering.

## Scope

- Refactor the non-prefix query path in `plugin_host.rs`
- Update `ResultChannel` in `search/types.rs` to carry and tag with source id
- Check if the prefix query path has the same pattern and unify if so
