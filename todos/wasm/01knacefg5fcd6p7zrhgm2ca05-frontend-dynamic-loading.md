# Frontend Dynamic Loading

Replace the static plugin component registry with dynamic resolution: extract frontend bundles from plugin archives to a cache directory, serve them via the `asset://` protocol, and load them via dynamic `import()`.

**Strategy doc:** §6.3 (extraction + asset protocol + dynamic import flow), §7 (plugin author build pipeline, SDK package), §3.3 (existing frontend plugin surface: views, inline views, settings component), §3.4 (existing Vite chunk splitting, `assetProtocol` scope)

**Status:** not started

**Depends on:** `01knacefg5fcd6p7zrhgm2ca01` (hello-world-integration), `01knacefg5fcd6p7zrhgm2ca03` (archive-source, for frontend asset extraction)

**Discussion needed:** SDK externalization strategy (§7.2): how are `@torchsnap/keybindings`, `@torchsnap/components`, `@torchsnap/types` resolved at runtime in dynamically loaded plugin bundles? Options: importmap in HTML, global variable injection, shared Vite chunk. Needs hands-on experimentation — decision deferred to this task.

**Notes:** `$APPDATA/**` is already in Tauri's `assetProtocol` scope. The `pluginComponent.tsx` lazy-loading infrastructure already supports dynamic `import()` factories. Cache invalidation: compare archive mtime or a hash field against the extracted cache; only re-extract on change (§6.3).
