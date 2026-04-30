# Structured error types for Tauri commands

## Problem

All `#[tauri::command]` functions currently serialize errors as opaque
`String` via `.map_err(|e| format!("{e:#}"))`. The frontend receives a
plain string on rejection and cannot distinguish error kinds, decide
whether to retry, or show context-appropriate messages.

## What's needed

- Define a structured error enum (or per-domain enums) for Tauri
  command errors on the Rust side, serializable via Serde.
- Return typed error variants instead of `Result<T, String>`.
- Mirror the error types on the TypeScript side so the typed invoke
  wrapper (see `01kn30th27x0arv9a5cekkwgpv-typesafe-invoke-wrapper.md`)
  can expose them to callers.

## Affected commands

All 8 current commands use `Result<_, String>`:
- `search`, `search_execute`, `plugin_message` (search domain)
- `frecency_stats`, `frecency_clear` (frecency domain)
- `launcher_hide`, `launcher_set_layout`, `control_subscribe` (infallible today, but should be consistent)

## Notes

- This is a prerequisite for actionable frontend error handling
  (see `01kmpk3scmw3h8vpptdfjnp2bv-frontend-backend-error-handling.md`).
- Once structured errors exist, the typed invoke wrapper's return types
  should be updated to include the specific error variants per command.
