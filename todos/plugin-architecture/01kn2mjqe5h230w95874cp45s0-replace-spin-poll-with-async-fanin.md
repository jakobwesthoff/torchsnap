# Replace Spin-Poll Loop with Async Fan-In

`plugin_host.rs` non-prefix query plugin path uses a `try_recv` / `yield_now`
polling loop to drain results from multiple query plugins. This is a busy-wait
approximation.

Replace with proper async fan-in:
- `futures::stream::select_all` over the receivers, or
- A `JoinSet` that collects results as tasks complete, or
- `tokio::select!` over a dynamic set

Also fix the one-removal-per-iteration issue: currently `swap_remove` handles
one disconnected receiver per outer loop iteration, requiring N full scans for
N simultaneously-finishing plugins.

Current behavior is fine with few plugins, but degrades with more and wastes
CPU cycles in the meantime.
