# README documents `just build` as a release build; the recipe defaults to debug

**Kind:** bug (documentation)
**Severity:** medium
**Area:** README.md, just/build.just

## Problem

`README.md:60-63` documents the build recipes as:

> - `just build` — full release build. Stages whitelisted gadgets
>   from `gadgets/bundled.toml` into `target/bundled-gadgets/`,
>   builds every in-tree gadget, then runs `tauri build`.
> - `just build profile=debug` — same flow with `tauri build --debug`.

The recipe (`just/build.just:15-19`) is:

```just
[arg('profile', long='release', value='release')]
build profile="debug":
    just stage-bundled-gadgets
    just build-gadgets
    bun run tauri build {{ if profile == "release" { "" } else { "--debug" } }}
```

Two mismatches:

1. **Default inverted.** The `profile` parameter defaults to
   `"debug"`, so bare `just build` runs `tauri build --debug` — a
   debug bundle, not the "full release build" the README claims.
   A release build requires `just build release` (positional) or
   `just build --release` (via the `[arg]` annotation).
2. **`just build profile=debug` is not a recipe argument.** In
   `just`, a `name=value` token on the command line is a variable
   override, not a recipe-parameter assignment; the recipe
   parameter keeps its default. The documented invocation only
   produces a debug build because debug already *is* the default —
   the syntax teaches readers a wrong mental model that breaks
   the moment they try `just build profile=release` (still
   builds debug: the override sets a variable the parameter
   shadows).

## Impact

Anyone following the README ships a debug bundle believing it is
a release build; `just build profile=release` silently produces
the wrong artifact.

## Suggested fix

Update `README.md` to document `just build` (debug, default),
`just build release` / `just build --release` (release), and drop
the `profile=debug` syntax. Alternatively flip the recipe default
to `release` if that matches the intended workflow, then fix the
README to match whichever is chosen.
