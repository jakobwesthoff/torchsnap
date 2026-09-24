---
kind: bug
severity: low
status: open
area: [src-tauri/src/gadgets/clipboard/formats.rs]
tags: [unconfirmed, concurrency]
---

# Display-text derivation re-reads the pasteboard — mismatch race and double image decode

## Problem

`extract_all_formats` captures each format from the clipboard,
then derives the display text by calling the clipboard getters
*again* (`formats.rs:74-93` and `derive_display_text`,
`formats.rs:183-207`). The section comment acknowledges the double
read as intentional decoupling: "The typed clipboard-rs getters
are called again here (cheap, data is still on the pasteboard)"
(`formats.rs:170-173`).

Two consequences:

1. **Capture/display mismatch race.** The pasteboard can change
   between the capture loop and `derive_display_text` (rapid
   successive copies; the watcher handler is still processing the
   previous change). The stored entry then persists format data
   from copy A with display text from copy B. The stored
   `content_hash` covers only the format data, so the wrong
   display text is permanent and the FTS index (built from
   display text, `schema.rs` triggers) indexes content the entry
   does not contain.

2. **Double image decode.** For image clipboard content,
   `read_format` calls `clipboard.get_image()` and PNG-encodes it
   (`formats.rs:147-158`), and `derive_display_text` calls
   `get_image()` again just to read dimensions
   (`formats.rs:199-204`). Clipboard images (screenshots) are
   routinely multi-megabyte; both calls transfer and decode the
   full image on the watcher thread per clipboard change. "Cheap"
   in the comment is not accurate for this format.

## Suggested fix

Derive the display text from the already-captured
`Vec<CapturedFormat>` instead of re-reading the pasteboard: text
and files formats already carry their bytes; for images, capture
the dimensions once at `read_format` time (clipboard-rs exposes
`get_size()` on the image it already returned) and thread them
through `CapturedFormat` or the capture result. That removes both
the race and the second decode without giving up the
capture/display separation.
