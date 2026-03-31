# Fix Calculator Unsafe AtomicBool Pattern

`calculator.rs` (lines 491-535) casts `*const AtomicBool` through `usize` to
make them `Send` across `spawn_blocking` thread boundaries. Three separate
settings watch threads use this pattern.

The safety argument (the `Arc<CalculatorPlugin>` outlives the thread) is sound,
but the pattern is unusual and fragile. It would break if the plugin lifetime
model ever changes.

Alternatives:
- Wrap the `AtomicBool`s in `Arc` and clone into the thread (simplest)
- Use `SettingsWatch<bool>` directly in the thread instead of converting to
  `AtomicBool` first
- Use a single settings watch thread that updates all three bools
