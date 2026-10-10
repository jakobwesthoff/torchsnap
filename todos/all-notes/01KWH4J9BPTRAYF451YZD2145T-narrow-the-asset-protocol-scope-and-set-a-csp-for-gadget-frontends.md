---
title: "Narrow the asset protocol scope and set a CSP for gadget frontends"
kind: bug
component: backend
impact: high
tags: [security]
---
# Narrow the asset protocol scope and set a CSP for gadget frontends

The Tauri config sets `csp: null` and an asset protocol scope that covers all of `$APPDATA`. Untrusted gadget JS runs at the host origin with full Tauri IPC, so a gadget can read every gadget's SQLite store and `settings.json` through `convertFileSrc` and `fetch`, and can load remote code with no restriction. Severity high: a reliable, no-user-interaction, cross-gadget and host secret-disclosure primitive.

## Progress (0.12.0)

The update and welcome windows, which show no gadget code, get a CSP in production builds from the Vite plugin in `vite/csp.ts` (hashes of inline scripts such as the theme preload, no remote sources). That plugin answers "What breaks" item 1 for any page it is applied to. The launcher, Settings and Developer Tools windows still run without a CSP, and the asset-protocol scope is unchanged.

```json
"security": {
  "csp": null,
  "assetProtocol": { "enable": true, "scope": ["$APPCACHE/**", "$APPDATA/**"] }
}
```

## Threat model

Gadget frontend bundles are third-party, untrusted JS. They are dynamically `import()`ed into the host webviews (`wasmPluginLoader.ts:61/77/92`, from `torchsnap-gadget://localhost/<id>/…`), run at the host origin, and have full Tauri IPC (`withGlobalTauri: true`). Gadget CSS is fetched from `torchsnap-gadget://` and injected as a same-origin `<style>` (`src/lib/gadgetCss.ts:24-41`). There is one capability file (`capabilities/default.json`) applying to `["main","settings","devtools"]`; it contains no per-window restriction of the asset protocol.

## Sub-issue (b): over-broad asset-protocol scope, confirmed, high

`$APPDATA`/`$APPCACHE` resolve to `path().app_data_dir()` / `path().app_cache_dir()`. Identifier `app.torchsnap`, so on macOS `$APPDATA` = `~/Library/Application Support/app.torchsnap`.

What lives under `$APPDATA` (all confirmed by source):

- **Every gadget's SQLite store.** Host-managed gadget state roots at `<app_data_dir>/gadget-home/<id>/` (`gadget_host.rs:251`). WASM DB: `gadget-home/<id>/sql/storage.sqlite3` (`wasm/bridge.rs:260,868,1158,1285`). Native clipboard DB: `gadget-home/clipboard-manager/sql/clipboard.sqlite3` (`gadgets/clipboard/mod.rs:300`, id `clipboard-manager` from `schema.rs:20`).
- **Frecency DB:** `<app_data_dir>/frecency.sqlite3` (`frecency/mod.rs:142`).
- **Settings store `settings.json`** (`lib.rs:654`, via `tauri-plugin-store`, resolved against `app_config_dir()`). On macOS `app_config_dir == app_data_dir`, so it is under `$APPDATA` and in scope. On Linux the two differ (`~/.config` vs `~/.local/share`), so `settings.json` falls outside `$APPDATA` there. The clipboard, frecency and gadget DBs are under `$APPDATA` on all platforms. Gadget settings are stored in `settings.json` under `gadgets.<id>.` (`settings/mod.rs:153`). For example, the ZeroTier gadget persists a pasted API auth token as `gadgets.zerotier.manualToken`.

The asset protocol is enabled globally and the scope is the only gate; no per-window capability narrows it. Any JS in the `main`/`settings` webview, including an untrusted gadget bundle, can call `convertFileSrc("<any $APPDATA or $APPCACHE path>")` and `fetch` the resulting `asset://localhost/…` (macOS) or `http://asset.localhost/…` (Windows) URL to read the raw bytes:

```js
const url = convertFileSrc(
  "/Users/<me>/Library/Application Support/app.torchsnap/gadget-home/clipboard-manager/sql/clipboard.sqlite3");
const bytes = await (await fetch(url)).arrayBuffer();   // full clipboard history
```

It can equally read another gadget's `sql/storage.sqlite3`, `frecency.sqlite3`, and (macOS) `settings.json` tokens. The clipboard DB is the highest-value target: clipboard history routinely contains passwords, 2FA codes, and tokens. Home-directory absolute paths are trivially derivable (fixed app-data root shape plus OS and username-adjacent info).

Severity **high**: a reliable, no-user-interaction, cross-gadget and host secret-disclosure primitive available to any untrusted gadget frontend using two always-available same-origin APIs (`convertFileSrc` and `fetch`). It defeats the per-gadget data isolation the storage layout (ADR 0017/0018/0019) is built to provide.

## Sub-issue (a): csp null, confirmed, high

No CSP is emitted on any webview. Untrusted gadget JS at host origin can load remote `<script>`/`import()`, remote styles, fonts and images, open arbitrary `connect-src` (fetch, WebSocket, `sendBeacon`) to any host, and use `eval`/`new Function`. Combined with full host IPC, a bundle can fetch remote second-stage code and exfiltrate anything it reads (including the DB bytes from (b)) with no restriction.

Gadget JS runs *same-origin* as the host. CSP is an origin-level control: it cannot isolate one gadget from another or from host IPC, and cannot stop same-origin asset-protocol reads. CSP's value is bounding **remote inclusion and remote exfiltration** and disabling `eval`. It is not a gadget-isolation boundary. Fixing (b) still requires the scope and scheme work below; CSP and scope are complementary, not substitutes. Severity is high given untrusted gadgets: without CSP a malicious bundle has an unrestricted remote-code-load and exfiltration channel at host origin.

## Suggested fix, part 2: minimal asset-protocol scope

Traced every `convertFileSrc` call site (only three exist):

- `ImagePreview.tsx:38` / `FileListPreview.tsx:65`: `convertFileSrc(imageFormat.path)`, where `imageFormat.path` is the clipboard gadget's `FileStorage` blob (`gadgets/clipboard/storage.rs:172-174` → `storage/file_storage.rs:110-112`), at `$APPDATA/gadget-home/clipboard-manager/files/<2hex>/<hash>.<ext>`.
- `Icon.tsx:80`: `convertFileSrc(value)` for bare-path `AssetIcon`s, which come from the host icon cache (`app_launcher.rs:146/208`, `system_preferences.rs:80/138` → `IconCache::ensure_icon`, `lib.rs:678-683`, `icons/icon_cache.rs:55-84`), at `$APPCACHE/icons/**`. All other `AssetIcon` values are `torchsnap-favicon://` URLs (the `://` branch bypasses `convertFileSrc`) or gadget-supplied absolute or qualified paths, which are rejected host-side (`wasm/bindings.rs:348-363`). No gadget-controlled path and no absolute *system* path reaches `convertFileSrc`.

So no system path outside app-data and cache is required. Minimal scope:

```json
"assetProtocol": {
  "enable": true,
  "scope": ["$APPCACHE/icons/**", "$APPDATA/gadget-home/*/files/**"]
}
```

- `$APPCACHE/icons/**` covers the icon cache and excludes the website-metadata cache DB (`$APPCACHE/website-metadata/metadata.sqlite3`).
- `$APPDATA/gadget-home/*/files/**` covers the clipboard blob dir (and any future gadget `FileStorage` `files/` subtree) while excluding every gadget's `sql/*.sqlite3`, `frecency.sqlite3`, and `settings.json`, which directly closes the (b) primitive. Tightest alternative: `"$APPDATA/gadget-home/clipboard-manager/files/**"` (the only current consumer).

## Suggested fix, part 3: concrete CSP

Verified webview needs: bundled app JS/CSS from `'self'`; gadget bundles via `import()` from `torchsnap-gadget:`; gadget CSS via `fetch()` from `torchsnap-gadget:` then injected `<style>`; images from `asset:`, `torchsnap-gadget:`, `torchsnap-favicon:`, `data:` (`Icon.tsx:70`); font `Inter-Variable.woff2` from `'self'`; mascot images from `'self'`. No webview-side `WebAssembly`/`eval`/`new Function` exists (gadget WASM runs host-side in wasmtime), so no `wasm-unsafe-eval` is needed. No gadget frontend performs a remote `fetch()` today, so a restrictive `connect-src` breaks nothing current and is exactly the exfiltration control wanted. Both scheme spellings are included because Tauri maps them per platform (macOS and Linux `://`, Windows `http://<scheme>.localhost`).

```
default-src 'self';
script-src 'self' torchsnap-gadget: http://torchsnap-gadget.localhost;
style-src 'self' 'unsafe-inline';
img-src 'self' data: asset: http://asset.localhost torchsnap-gadget: http://torchsnap-gadget.localhost torchsnap-favicon: http://torchsnap-favicon.localhost;
font-src 'self';
connect-src 'self' ipc: http://ipc.localhost asset: http://asset.localhost torchsnap-gadget: http://torchsnap-gadget.localhost torchsnap-favicon: http://torchsnap-favicon.localhost;
object-src 'none';
base-uri 'self';
frame-ancestors 'none'
```

- `script-src`: deliberately no `'unsafe-inline'` and no `'unsafe-eval'`. Tauri auto-injects hashes or nonces for its own IPC bootstrap when a CSP is present. `torchsnap-gadget:` is required for the gadget `import()`.
- `connect-src` includes `ipc:` / `http://ipc.localhost` for Tauri v2 IPC and `torchsnap-gadget:` for the CSS `fetch()`. Keep `asset:` for any programmatic fetch of clipboard or icon bytes.
- `style-src 'unsafe-inline'` is unavoidable: inline `<style>` in the HTML entries, pervasive React `style={{…}}`, and the gadget CSS injected as raw `<style>` textContent. This is an accepted concession, since style injection cannot exfiltrate the way script can.

What breaks and requires action:

1. The inline pre-paint theme `<script>` in `settings.html` / `devtools.html` is blocked by `script-src 'self'`. Externalize it to a `'self'` module (preferred) or add its `'sha256-…'` hash. Do not add `'unsafe-inline'` to `script-src`.
2. Dev mode (Vite on `http://localhost:1420`) needs inline scripts, `'unsafe-eval'`, and `ws://localhost:1420` in `connect-src`, and a different origin than prod. `tauri.conf.json` has a single `csp` field, so a prod-strict CSP breaks `tauri dev` unless a dev config override loosens `script-src` and `connect-src` for `localhost` and `ws:`. This is a required build-config change, not a code change.
3. Any gadget doing remote `fetch()` or `import()` from webview JS breaks (none today). Document it as an intentional gadget-authoring constraint: gadget frontends route network I/O through the host WASM network capability, not raw webview `fetch`.
4. Any gadget using `eval` or `new Function` breaks (none observed).

## Suggested fix, part 4: prefer custom schemes (durable)

The asset protocol is a generic filesystem reader gated only by a path glob. Both real consumers read host-managed files at host-known roots, which a custom URI scheme with a Rust handler does better: the handler can enforce root containment, extension allowlisting, and key validation instead of trusting a glob. The project already runs this pattern twice (`torchsnap-gadget://`, `torchsnap-favicon://`). Direction:

- Serve icon-cache webp and clipboard blobs through a host custom scheme (extend `torchsnap-gadget://` or add a narrow `torchsnap-asset://`) whose handler canonicalizes and containment-checks against the icon-cache root and clipboard `files/` root only.
- Emit those scheme URLs host-side instead of absolute paths (`AssetIcon` already passes `://` URLs straight through in `Icon.tsx:80`; the clipboard schema would emit a scheme URL).
- Then set `"assetProtocol": { "enable": false }`, removing the `convertFileSrc` disclosure primitive entirely.

Interim versus target: ship the tightened two-entry scope now (removes the DB and settings disclosure immediately, minimal change). Treat the custom-scheme migration plus `enable: false` as the durable fix that eliminates the generic filesystem-read surface.

## Key files

`tauri.conf.json:16-22`; `capabilities/default.json`; `src/components/Icon.tsx:70,80`; `src/gadgets/clipboard/detail/ImagePreview.tsx:38`, `FileListPreview.tsx:65`; `src/lib/gadgetCss.ts:24-41`; `src/gadgets/wasmPluginLoader.ts:40-92`; `src-tauri/src/gadgets/clipboard/mod.rs:300-304` + `schema.rs:20`; `src-tauri/src/storage/file_storage.rs:110-112`; `src-tauri/src/icons/icon_cache.rs:55-84` + `lib.rs:678-683`; `src-tauri/src/frecency/mod.rs:142`; `src-tauri/src/wasm/bindings.rs:336-363`; `src-tauri/src/gadget_host.rs:251`.

## Relations

- Relates to: [Gadget CSS can break out of its @scope block into global rules](01KWH4J9BPTRAYF451YZD2145S-gadget-css-can-break-out-of-its-scope-block-into-global-rules.md), this todo is where the resource-fetch and isolation fixes for gadget CSS land
