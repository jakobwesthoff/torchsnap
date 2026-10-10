---
title: "Return structured error types from Tauri commands"
kind: feature
component: backend
---
# Return structured error types from Tauri commands

Every `#[tauri::command]` serializes its errors as an opaque string via `.map_err(|e| format!("{e:#}"))`. The frontend receives a plain string on rejection and cannot tell the error kinds apart, decide whether to retry, or show a message that fits the case. Structured, serializable error types fix this on both the Rust side and the TypeScript side. They are also the prerequisite for actionable frontend error handling, which is the first consumer.

## What's needed

- Define a structured error enum (or per-domain enums) for Tauri command errors on the Rust side, serializable via Serde.
- Return typed error variants instead of `Result<T, String>`.
- Mirror the error types on the TypeScript side so a typed invoke wrapper can expose them to callers.

## Affected commands

All 8 current commands use `Result<_, String>`:

- `search`, `search_execute`, `gadget_message` (search domain)
- `frecency_stats`, `frecency_clear` (frecency domain)
- `launcher_hide`, `launcher_set_layout`, `control_subscribe` (infallible today, but should be consistent)

## Notes

- Once structured errors exist, the typed invoke wrapper's return types should be updated to include the specific error variants per command.
