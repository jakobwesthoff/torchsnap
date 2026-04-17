# Dev Plugin Search Path — Future Alternatives

The bundled/user-installable plugins work (see
`.claude/plans/cosmic-seeking-rivest.md`) ships with a single, minimal
approach for loading plugins during development:

> Debug builds (`cfg!(debug_assertions)`) additionally scan
> `env!("CARGO_MANIFEST_DIR")/../plugins` as a third search root. The
> path exists only while the binary runs from the cargo build tree,
> which is exactly the dev case.

This todo captures the alternative approaches we explicitly considered
and shelved, so a future revisit has the design space preserved.

**Status:** needs discussion

**Dependency:** the v1 approach needs to be in production long enough
to reveal whether an escape hatch is actually needed.

## Current v1 approach

- Zero configuration; works automatically with `bun run tauri dev`.
- Couples "dev plugin path" to "debug build profile" — if someone ever
  builds a debug-profile release for profiling, the dev path
  activates. Low concern in practice.
- First use of `cfg!(debug_assertions)` in the codebase. Precedent-
  setting but bog-standard Rust.
- No runtime override. Integration tests drive the loader directly
  with explicit paths, so the lack of an override is not a test-infra
  concern.

## Alternatives considered

### A. Environment variable (`TORCHSNAP_DEV_PLUGIN_PATH`)

- **Pros**: explicit, works in release builds for troubleshooting,
  test-friendly.
- **Cons**: adds friction to everyday `bun run tauri dev` — the
  developer must remember to set it. Mitigation: `.env` or a Just
  recipe that exports it.

### B. CLI argument (`torchsnap --dev-plugins=/path`)

- **Pros**: explicit, runtime.
- **Cons**: unergonomic for a GUI app; requires CLI parsing infra;
  not how users launch the app. App entry today doesn't parse argv.

### C. Dev config file (e.g. `torchsnap.dev.toml` in repo root)

- **Pros**: checked-in, zero-friction for contributors cloning the
  repo.
- **Cons**: over-engineered for a single boolean decision;
  introduces a new config format just for dev.

### D. CWD-relative scan (`./plugins/` in debug builds)

- **Pros**: no compile-time coupling to `CARGO_MANIFEST_DIR`; the
  binary works from any checkout path.
- **Cons**: fragile — depends on where the binary is invoked from;
  easy to surprise a developer who runs `torchsnap` from their home
  directory.

### E. Additive env-var override (best of v1 + A)

- Default to the debug-assertions path, but let an env var provide
  an **additional** root (not a replacement).
- **Pros**: zero-config default preserved; runtime escape hatch when
  needed (e.g., a plugin author wanting to test against a release
  build or a different checkout).
- **Cons**: more surface to document and test; risk of path
  ambiguity when both are set.

## Decision criteria for revisit

Revisit when one of these is true:
- Plugin authors repeatedly ask for a way to point a release build at
  a development plugin directory.
- CI needs to run the full app against a plugin checkout in a path
  that differs from the repo layout.
- The `cfg!(debug_assertions)` coupling causes a real surprise (e.g.,
  a profiled debug release picks up the dev dir unexpectedly).

Until then, the v1 approach stays.
