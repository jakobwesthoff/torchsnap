# Execute-triggered plugin custom UI

ADR 0013 (topic 1) defines a second activation path for plugin custom
UI: `execute()` returns a `PostAction::ShowCustomUI` variant, which
tells the frontend to mount the plugin's component as a "drill-in"
from the current result list.

This requires:
- New `ShowCustomUI` variant on the `PostAction` enum
- Navigation stack in the host: snapshot current state (query, results,
  selected index) before mounting the plugin component
- `goBack()` restores the snapshot and unmounts the plugin
- Escape at the plugin layer calls `goBack()` when nothing internal
  to unwind

Example use case: file search plugin shows results in standard list,
user hits Enter on a folder, plugin takes over to show a custom file
browser.

Not needed for the emoji grid (prefix-activated only). Build when the
first drill-in plugin is implemented.
