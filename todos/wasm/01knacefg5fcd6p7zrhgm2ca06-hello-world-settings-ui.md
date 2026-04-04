# Hello-World Settings UI

Extend the hello-world plugin with a settings panel and a custom frontend view. Proves settings reactivity (`on-setting-changed`) and the frontend bundle loading pipeline end-to-end.

**Strategy doc:** §5.2 (`on-setting-changed` guest export in `lifecycle` interface), §4.2 (`[settings]` flat defaults, `[frontend]` bundle declarations), §3.3 (views, settings component surface)

**Status:** not started

**Depends on:** `01knacefg5fcd6p7zrhgm2ca05` (frontend-dynamic-loading), `01knacefg5fcd6p7zrhgm2ca04` (plugin-logging, for debugging frontend ↔ backend communication)

**Notes:** The hello-world plugin will need a `manifest.toml` update to declare `[settings]` defaults and `[frontend]` bundle paths. The WIT `lifecycle` interface needs `on-setting-changed` added. Success criteria: change a setting in the settings window → host calls `on-setting-changed` → plugin logs the change; custom view renders in launcher.
