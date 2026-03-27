# Clipboard: Scrollable detail preview with more content

**Priority: after lazy-detail-loading is settled**

The detail preview panel currently clips content at the visible area
(`overflow-hidden`) and the backend caps text at 1000 chars. This is
intentional for the initial implementation to avoid scrollbars, but
limits usefulness for longer clipboard entries.

## Goals

- Make the detail preview panel scrollable (custom scrollbar styling
  to match the app's visual language, or hidden scrollbar with
  scroll-on-drag / keyboard scroll).
- Increase the backend `DETAIL_PREVIEW_MAX_CHARS` cap — evaluate
  whether 5000 or uncapped is reasonable given typical clipboard
  sizes and IPC overhead.
- Consider a "show full content" expand gesture if entries can be
  very large (e.g., pasted log output).

## Considerations

- The preview is `select-none pointer-events-none` right now to
  prevent accidental text selection. Scrolling would require
  re-enabling pointer events on the container while keeping text
  non-selectable.
- Evaluate whether the virtual scroll / windowed approach makes
  sense for very long text, or if native overflow scroll is fine
  for the detail panel since only one entry is shown at a time.
