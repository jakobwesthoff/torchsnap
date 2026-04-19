# Add `reveal-path` to the `opener` host WIT interface

## Context

The `opener` host interface (`wit/torchsnap-plugin.wit`) currently exposes
only `open-url`. A `reveal-path` function — "Show in Finder/Explorer" — was
originally proposed alongside it but deferred until `app-launcher` conversion
is underway.

The host-side plumbing is identical to `open-url`: both go through
`tauri_plugin_opener::OpenerExt`, so the implementation cost is low once
the interface is extended.

## Target

1. Add `reveal-path` to the `opener` interface in `wit/torchsnap-plugin.wit`:
   ```wit
   interface opener {
     open-url(url: string) -> result<_, string>;
     reveal-path(path: string) -> result<_, string>;
   }
   ```
2. Implement the host handler in `src-tauri/src/wasm/host/opener.rs`.
3. Expose in the plugin SDK if applicable.

## Security notes

- WASM plugins have no direct filesystem access, so any path passed to
  `reveal-path` must originate from the plugin author (hardcoded), user
  settings, or a future `http` response. The last case is worth monitoring
  when `http` is also in use.
- Log all calls with plugin ID + path for auditability.

## Blocked-by / enables

- Depends on: `todos/wasm/*-wasm-host-http-opener-interfaces.md` (base
  `opener` interface must land first)
- Enables: `app-launcher` WASM conversion
