---
kind: refactor
status: open
---

# Migrate type-alias caps to full struct implementations

## Context

Three caps are currently type aliases pointing to their underlying
implementations rather than proper cap structs with encapsulated behavior:

- `SettingsCap = crate::settings::GadgetSettings` (`caps/settings.rs`)
- `FrecencyCap = crate::frecency::GadgetFrecency` (`caps/frecency.rs`)
- `IconCacheCap = crate::icons::IconCache` (`caps/icon_cache.rs`)

These were left as aliases during the cap type migration because the
underlying types already had well-structured APIs and no permission
surface beyond binary access. However, this breaks the pattern
established by the other caps (OpenerCap, HttpCap, FilesystemCap, etc.)
where the cap struct owns the implementation and defines its own public
interface.

## Why this matters

1. **Inconsistent API surface.** Alias caps expose the full API of the
   underlying type. A proper cap struct would expose only the methods
   relevant to the capability contract, hiding implementation details.

2. **No room for permission logic.** If any of these caps gain
   fine-grained permissions in the future (e.g., read-only vs read-write
   settings, scoped icon cache access), the alias pattern has no place
   to put the checks.

3. **Testing difficulty.** `GadgetSettings` and `GadgetFrecency` depend
   on Tauri runtime infrastructure (`Store<Wry>`, `FrecencyStore`). A
   proper cap struct could accept the dependency via constructor
   injection with a trait, making it testable with mocks.

4. **Leaky abstraction.** Consumers import `SettingsCap` but get
   `GadgetSettings<Wry>` with a generic parameter and all its methods.
   A cap struct would present a clean, non-generic interface.

## TODO

For each alias cap:

1. Evaluate what API surface the cap should expose vs what should be
   hidden.
2. Create a proper struct wrapping the underlying type.
3. Expose only the methods that are part of the capability contract.
4. Consider whether the constructor can accept a trait for testability
   (e.g., a `SettingsStore` trait instead of `Arc<Store<Wry>>`).
5. Add unit tests.

## Caps to migrate

- **SettingsCap**: wraps `GadgetSettings`. Exposes `get<T>()`,
  `get_raw()`. Could gain `set()` in the future.
- **FrecencyCap**: wraps `GadgetFrecency`. Exposes `score()`,
  `scores()`, `apply_scores()`, `top_items()`, `is_enabled()`. Note:
  `record()` is host-managed (called by `GadgetHost::execute`), not
  gadget-facing.
- **IconCacheCap**: wraps `IconCache`. Exposes `ensure_icon()`,
  `cleanup()`. Native-only (no WIT host import).
