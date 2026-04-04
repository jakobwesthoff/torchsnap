# Calculator Plugin Conversion

Convert the `calculator` plugin from native Rust to a WASM component. First real plugin migration. Exercises SQL storage, clipboard write, settings, and both `CustomUI` and `InlineUI` result types.

**Strategy doc:** §8.2 Phase 1 (calculator: low complexity, good first candidate), §8.3 (step-by-step conversion process), §5.3 (host imports needed: `torchsnap:host/storage`, `torchsnap:host/clipboard`, `torchsnap:core/settings`)

**Status:** not started

**Depends on:** `01knacefg5fcd6p7zrhgm2ca06` (hello-world-settings-ui), `01knacefg5fcd6p7zrhgm2ca03` (archive-source, for producing a `.torchsnap` file)

**Notes:** Follow §8.3 exactly: extend WIT first, implement host imports, port logic, bundle frontend, integration test with both native and WASM versions active, then remove native. The `CustomUI` and `InlineUI` return variants require WIT extensions beyond the current hello-world subset — these will need to be added to the WIT and host bindings as part of this task. Remove the native calculator implementation only after WASM version is validated.
