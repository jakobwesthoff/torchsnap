---
title: "Install, uninstall and reload gadgets without an app restart"
kind: feature
component: gadget-host
status: needs-discussion
tags: [migration]
---
# Install, uninstall and reload gadgets without an app restart

Three related capabilities (hot install, hot uninstall and hot reload) all depend on the same infrastructure: a runtime gadget-registration API on `GadgetHost` that does not rely on `&mut self` and on the gadget set being frozen after setup, which the current startup assumes.

It replaces the restart-based flow of ADR 0051. Installs, updates and uninstalls are recorded as pending changes, shown in Settings → Gadgets with Undo and a "Restart now" bar, and take effect on the next start (`src-tauri/src/gadget_install/`). That is the known-good fallback this work replaces.

## The three capabilities

- **Hot install.** A user drops a `.torchsnap` into `$APPDATA/gadgets/` (via the install UI). The host registers it, instantiates it, and makes it visible in the launcher and settings without an app restart. The restart banner collapses into a success toast.

- **Hot uninstall.** A user clicks Uninstall on a user gadget. The host disables it, drops the WASM instance, closes the SQLite connection, unregisters any global shortcuts, stops any scheduled tasks, removes the archive and `gadget-home/<id>/`, and strips its settings keys. React views owned by the gadget unmount cleanly.

- **Hot reload (developer).** In debug builds only: a filesystem watcher watches `CARGO_MANIFEST_DIR/../gadgets/` (and any dev symlinks in `$APPDATA/gadgets/`). On a change to `manifest.toml` or the `.wasm` artifact, the host invalidates the `WasmRuntime` component cache for that ID, replaces the running instance and notifies the frontend so views remount. Gadget-author productivity depends on this.

## Shared infrastructure

- **Runtime registration API on `GadgetHost`.** Today `register` takes `&mut self`, and the host is moved into Tauri state after setup. It needs interior mutability (`RwLock<GadgetMap>` or similar) and a surface for `register`, `unregister` and `replace_instance` that is callable after `initialize_and_start`.
- **Lifecycle hooks:** a clean `disable`, then drop the instance, drop the `WasmGadgetBridge`, close the SQL connection, stop cron tasks and release shortcuts. Each needs an explicit teardown path.
- **`WasmRuntime` component-cache invalidation** keyed by gadget ID, so a reload actually re-reads the WASM bytes from disk.
- **Scheduler coordination.** Gadget cron tasks must be told to stop before a reload or uninstall, otherwise a stale instance keeps firing.
- **Frontend registry updates.** `src/gadgets/registry.ts` must become live-updatable. Existing views for the affected gadget must unmount and remount (a Suspense boundary or an explicit unmount signal).
- **Settings coherence.** Uninstall removes keys. Reload preserves them, so gadget state survives an iteration cycle.
- **SQLite handle lifecycle.** Reload reopens the same DB. Uninstall closes the connection before deleting files (Windows will complain otherwise, and so will macOS in a future port).

## Open risks

- **React state is lost on reload.** A gadget view holding local state gets reset. That is acceptable for dev hot reload, but it may be surprising for a hot install of an unrelated gadget if the settings window re-renders broadly. Scope the remount narrowly.
- **In-flight search queries.** A search result streaming from a gadget that is being unloaded mid-flight needs cancel channels. Do not block on stale senders.
- **Long-running gadget tasks** (spawned work, pending cron tasks, open WASM component instances waiting on a host call) must be told to cancel, not merely abandoned.
- **File watcher debouncing.** Build pipelines write files in bursts (copy `.wasm`, then touch `manifest.toml`). The watcher must debounce and coalesce to avoid mid-build reloads.
- **Race: reload during enable.** If a reload fires while the gadget is in the middle of `enable()`, the state machine must serialize reliably.

## UI implications

- The Gadgets settings panel drops its restart banner once hot install and uninstall land. A small notification replaces it.
- In debug builds, a "dev watcher active" indicator is useful so an author sees reloads happening.
