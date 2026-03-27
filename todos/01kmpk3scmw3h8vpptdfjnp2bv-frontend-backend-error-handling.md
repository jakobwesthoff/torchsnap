# Handle backend errors in the frontend

Currently all `invoke` and `sendMessage` calls in the frontend either
ignore errors entirely (fire-and-forget) or silently swallow them.
Failed clipboard pastes, broken searches, and plugin message errors
are invisible to the user.

## What's needed

- A lightweight error notification system (e.g., transient toast or
  inline error banner in the launcher).
- Consistent error handling at call sites: `invoke` / `sendMessage`
  rejections should surface through the notification system rather
  than being caught and discarded.
- Consider which errors are actionable (user can retry) vs. internal
  (just log), and only surface actionable ones.

## Affected call sites

- `sendMessage("paste", ...)` — clipboard write failure
- `sendMessage("delete", ...)` — entry deletion failure
- `sendMessage("subscribe", ...)` — subscription setup failure
- `invoke("execute_action", ...)` — action execution failure
- `invoke("search", ...)` — search failures (currently silent)
