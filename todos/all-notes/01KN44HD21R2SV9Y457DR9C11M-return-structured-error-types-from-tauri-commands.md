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

About 40 `#[tauri::command]` functions exist (`rg '#\[tauri::command\]' src-tauri/src`), in `commands/mod.rs`, `lib.rs`, `welcome.rs`, `updates/mod.rs`, `gadget_install/` and `wasm/logging/commands.rs`. Those that can fail return `Result<_, String>`, for example `frecency_clear`, `welcome_finish` and `update_install`. List them again when starting, and decide per domain which get a typed error. Infallible ones such as `launcher_hide` and `control_subscribe` only need to stay consistent.

## Notes

- Once structured errors exist, the typed invoke wrapper's return types should be updated to include the specific error variants per command.
