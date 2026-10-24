---
title: "A website metadata test races its subscribers against the leader"
kind: bug
component: backend
impact: low
origin: implementation
tags: [testing, concurrency]
---
# A website metadata test races its subscribers against the leader

`network::website_metadata::service_tests::leader_panic_unblocks_subscribers_with_unreachable` failed once in a `just fullcycle` on macOS on 2026-10-24: subscriber a got a `JoinError::Panic` instead of `Unreachable`. A rerun passed. The test orders its tasks with a fixed sleep, so a loaded machine can break it.

## Problem

Read in the code, not reproduced. The test (`src-tauri/src/network/website_metadata/service_tests.rs`, around line 356) starts the leader's blocking lookup, which panics 100 ms in through `set_test_panic_after`, then sleeps 20 ms and starts two subscriber lookups (line 371):

```rust
// Wait for the leader to register before launching subscribers.
tokio::time::sleep(std::time::Duration::from_millis(20)).await;
```

If the leader's `spawn_blocking` task has not registered its in-flight slot within those 20 ms, subscriber a finds no leader, becomes the leader itself and hits the same test panic. That matches the observed `JoinError::Panic` for subscriber a.

## Suggested fix

Start the subscribers only after the leader has registered: wait on a signal from the in-flight registration (a test hook, a channel or a barrier) instead of the sleep, as the project's rules ask for tests of ordering. Run it several times before it counts as green.
