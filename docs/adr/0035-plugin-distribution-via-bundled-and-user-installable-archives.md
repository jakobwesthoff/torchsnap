# 35. Plugin distribution via bundled and user-installable archives

Date: 2026-04-17

## Status

Accepted

Amended by [42. Rename plugins to gadgets](0042-rename-plugins-to-gadgets.md)

Amended by [51. Install gadgets through a review queue with staged copies](0051-install-gadgets-through-a-review-queue-with-staged-copies.md)

## Context

The WASM plugin system reached a point where two distribution
concerns had to be solved together:

1. **Bundled plugins.** A curated subset of plugins (starting with
   the calculator, which is migrating away from native Rust) needs
   to ship inside the application bundle so the out-of-the-box
   experience matches what users expect from a launcher — the host
   cannot wait for users to install core functionality.
2. **User-installable plugins.** The whole point of a WASM plugin
   system is that third parties (or the user themselves) can ship
   their own plugins without rebuilding the host. Users need a way
   to install a `.torchsnap` archive they downloaded, and to
   uninstall it later.

Before this change the loader scanned a single hardcoded path —
`env!("CARGO_MANIFEST_DIR")/../plugins` — with a TODO noting that a
real discovery story was pending. Nothing was bundled into the
Tauri `.app`; nothing could be added at runtime.

A related constraint surfaced during design: WASM plugins' SQL
storage lived at `<app_data_dir>/plugins/<plugin-id>/storage.db`,
which would collide with a plan to treat `<app_data_dir>/plugins/`
as the install target for `.torchsnap` archives. The fix — moving
host-managed plugin state to a separate root — is documented
inline in the storage-path migration commit.

## Decision

### Three search roots, precedence-ordered

The loader scans three roots, in this precedence order:

1. **System** — `<resource_dir>/plugins/`. Populated by Tauri's
   bundler from `target/bundled-plugins/`, which a new
   `stage-bundled-plugins` Just recipe fills based on
   `plugins/bundled.toml`. Tagged `PluginSourceKind::System`.
2. **Dev** — `<CARGO_MANIFEST_DIR>/../plugins/`. Debug builds only
   (`cfg(debug_assertions)`); the branch is compiled out of
   release artifacts entirely. Tagged `PluginSourceKind::Dev`.
3. **User** — `<app_data_dir>/plugins/`. The install target for
   `.torchsnap` archives dropped through the Plugins settings
   panel. Tagged `PluginSourceKind::User`.

Per-root, an archive entry (`foo.torchsnap`) wins over a sibling
directory with the same stem (`foo/`). Cross-root, the first root
that registers a given plugin id wins; later duplicates are skipped
with a warning logged. The install flow rejects colliding ids up
front so the fallback is defensive rather than routine.

### On-disk layout

```
<resource_dir>/plugins/
    calculator.torchsnap            # system

<app_data_dir>/plugins/
    <id>.torchsnap                  # user-installed archive
    <id>/                           # dev-style user plugin dir

<app_data_dir>/plugin-home/
    <id>/
        sql/
            storage.sqlite3         # host-managed per-plugin state
        # future: files/, cache/, …
```

Plugin **code** lives under `plugins/`; plugin **state** lives
under `plugin-home/`. Splitting the two keeps the `plugins/`
directory owned entirely by the install/uninstall flow.

### Whitelist-driven bundling

`plugins/bundled.toml` lists ids shipped with release bundles, one
per line. Missing from the list = not bundled. The
`stage-bundled-plugins` recipe rebuilds each listed plugin and
copies its `.torchsnap` into `target/bundled-plugins/`, which
Tauri picks up through a new `resources` entry in
`tauri.conf.json`. A release without `bundled.toml` is a valid
state — it ships zero system plugins.

### Install / uninstall semantics (restart-required)

Install accepts any `.torchsnap` via file picker or drag-drop,
opens it with `ArchiveSource::open` (which parses the manifest and
runs the path-traversal guard), rejects collision with any
existing `PluginSourceKind`, and atomically copies the archive
into `<app_data_dir>/plugins/<id>.torchsnap` via a temp-then-rename
dance.

Uninstall rejects anything that is not a `User` plugin. It removes
the archive, any sibling unpacked directory, the
`plugin-home/<id>/` state tree, and the plugin's settings keys
(`enabled.<id>` plus every `plugins.<id>.*` key — exact-match plus
prefix so an id that textually prefixes another cannot strip the
longer id's state).

Both commands return `requiresRestart: true` because
`PluginHost::register` takes `&mut self` and the host is moved
into Tauri state after setup. A hot lifecycle story — install,
uninstall, and dev-reload without restart — is tracked in
`todos/gadget-host/wasm/01kpdsvj5at6agxst1jva5eeva-gadget-hot-lifecycle.md`.

### ID-collision policy: reject, don't override

User installs that collide with any existing plugin id (Builtin,
System, Dev, or already-installed User) are rejected with a
kind-specific error. An explicit "override a system plugin" path
is out of scope for v1; it is designed in
`todos/gadget-host/wasm/01kpdqsg7zmsrh2xssktpb5trb-system-gadget-override.md`.

## Consequences

### What this enables

- Release builds ship a curated set of plugins that Just Work out
  of the box. `calculator` is the first; others can be added by
  editing one file.
- Users can install third-party `.torchsnap` archives from the
  settings panel without rebuilding the host.
- Plugin authors get a stable discovery story for dev
  (`CARGO_MANIFEST_DIR/../plugins`, zero config) and a predictable
  install target for release (`<app_data_dir>/plugins/`).
- System vs user separation makes it trivial to reason about which
  plugins an upgrade will update and which ones survive.

### Trade-offs

- **Restart-required for now.** Install/uninstall prompts a restart.
  A hot path is follow-up work; the architectural shift — turning
  `PluginHost::register` into a runtime-callable API — is
  non-trivial and deserves its own ticket.
- **First use of `cfg!(debug_assertions)` in the codebase.** The
  dev path is gated on the debug-build profile. An env-var-based
  alternative is captured in a dedicated todo, to revisit when
  plugin authors actually hit the current design's limits.
- **No override flow.** A user who wants to replace a bundled
  plugin with their own must rename the id. The tradeoff is
  preserving the forking use case (same id inherits DB and
  settings) against the potential confusion of silent shadowing.
  Deferred rather than silently allowed.

### Cross-platform

Tauri's `resource_dir()` abstracts over platform bundle layouts
(`Contents/Resources/` on macOS, AppImage/.deb payload on Linux,
alongside the `.exe` on Windows). The staging recipe and
`tauri.conf.json` entry are platform-agnostic; we only smoke-test
macOS in v1.
