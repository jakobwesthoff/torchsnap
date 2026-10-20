---
title: "Clipboard capture can mix the formats of two quick copies"
kind: bug
component: gadgets
impact: low
origin: review
tags: [concurrency]
---
# Clipboard capture can mix the formats of two quick copies

The clipboard watcher reads every format of a change one after the other (`formats::extract_all_formats`, called from `process_clipboard_change` in `src-tauri/src/gadgets/clipboard/watcher.rs`) without checking that the pasteboard did not change in between. Two copies in quick succession can then produce one history entry that holds formats from both. Found on 2026-10-20 while fixing the display-text race (commit 359815c4, which removed the second pasteboard read for the display text but not this one), read in the code, not reproduced.

## Impact

Rare. Pasting such an entry can paste the text of one copy and the image or files of another, depending on which format the target app asks for.

## Suggested fix

Read the pasteboard's `changeCount` before and after the capture loop and discard (or retry once) a capture whose count changed. Test it with the fake clipboard the formats tests already use, changing the content between two format reads.
