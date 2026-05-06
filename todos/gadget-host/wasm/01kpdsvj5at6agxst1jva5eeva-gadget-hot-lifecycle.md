# Gadget Hot Lifecycle — Hot Install, Hot Uninstall, Hot Reload

Three related capabilities that all unblock on the same piece of
infrastructure: a runtime gadget-registration API on `GadgetHost` that
does not rely on `&mut self` and the "gadget-set-frozen-after-setup"
assumption baked into current startup.

**Status:** needs discussion

**Depends on:** the bundled/user-installable gadgets plan
(`.claude/plans/cosmic-seeking-rivest.md`) landing first — that ships
`install_gadget_archive` and `uninstall_user_gadget` with an explicit
"restart required" banner, which is the known-good fallback this work
replaces.

## The three capabilities

- **Hot install.** A user drops a `.torchsnap` into `$APPDATA/gadgets/`
  (via the install UI). The host registers it, instantiates it, and
  makes it visible in the launcher and settings without an app
  restart. The restart banner collapses into a success toast.

- **Hot uninstall.** A user clicks Uninstall on a user gadget. The host
  disables it, drops the WASM instance, closes the SQLite connection,
  unregisters any global shortcuts, stops any scheduled tasks, removes
  the archive + `gadget-home/<id>/`, and strips its settings keys.
  React views owned by the gadget unmount cleanly.

- **Hot reload (developer).** In debug builds only: a filesystem watcher
  watches `CARGO_MANIFEST_DIR/../gadgets/` (and any dev symlinks in
  `$APPDATA/gadgets/`). On a change to `manifest.toml` or the `.wasm`
  artifact, the host invalidates the `WasmRuntime` component cache for
  that ID, replaces the running instance, and notifies the frontend so
  views remount. Gadget-author productivity depends on this.

## Shared infrastructure

- **Runtime registration API on `GadgetHost`.** Today `register` takes
  `&mut self` and the host is moved into Tauri state after setup.
  Needs interior mutability (`RwLock<GadgetMap>` or similar) and a
  surface for `register` / `unregister` / `replace_instance` callable
  after `initialize_and_start`.
- **Lifecycle hooks:** clean `disable` → drop instance → drop
  `WasmGadgetBridge` → close SQL connection → stop cron tasks →
  release shortcuts. Each of these needs an explicit teardown path.
- **`WasmRuntime` component-cache invalidation** keyed by gadget ID,
  so reload actually re-reads the WASM bytes from disk.
- **Scheduler coordination.** Gadget cron tasks must be told to stop
  before a reload / uninstall, otherwise a stale instance keeps firing.
- **Frontend registry updates.** `src/gadgets/registry.ts` must become
  live-updatable; existing views for the affected gadget must unmount
  and remount (Suspense boundary or an explicit unmount signal).
- **Settings coherence.** Uninstall removes keys; reload preserves them
  so gadget state survives an iteration cycle.
- **SQLite handle lifecycle.** Reload reopens the same DB; uninstall
  closes the connection before deleting files (Windows will complain
  otherwise, and so will we on macOS in a future port).

## Open risks

- **React state is lost on reload.** A gadget view holding local state
  gets reset. Acceptable for dev hot-reload; may be surprising for hot
  install of an unrelated gadget if the settings window re-renders
  broadly. Scope the remount narrowly.
- **In-flight search queries.** A search result streaming from a gadget
  that is being unloaded mid-flight — cancel channels, don't block on
  stale senders.
- **Long-running gadget tasks** (spawned work, pending cron tasks, open
  WASM component instances waiting on a host call) must be told to
  cancel, not merely abandoned.
- **File watcher debouncing.** Build pipelines write files in bursts
  (copy `.wasm` → touch `manifest.toml`); the watcher must debounce
  and coalesce to avoid mid-build reloads.
- **Race: reload during enable.** If a reload fires while the gadget is
  in the middle of `enable()`, state machine must serialize reliably.

## UI implications

- The Gadgets settings panel drops its restart banner once hot install
  / uninstall land. A small notification replaces it.
- In debug builds, a "dev watcher active" indicator is useful so an
  author sees reloads happening.
