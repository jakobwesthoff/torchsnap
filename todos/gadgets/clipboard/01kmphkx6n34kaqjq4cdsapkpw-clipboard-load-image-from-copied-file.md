---
kind: improvement
status: open
---

# Clipboard: Load image content from copied files

When a file is copied from Finder and it's an image (PNG, JPEG, etc.),
we currently skip the image capture entirely because macOS puts the
file's type icon on the clipboard as the "image" representation.

Instead, we should:

1. Detect that the clipboard contains files via `ContentFormat::Files`
2. Check if any of the file paths point to image files (by extension
   or by reading the magic bytes)
3. Load the actual image data from disk and store it in FileStorage
   as we would for a directly copied image
4. Still skip the `get_image()` call since that returns the file icon,
   not the content

This gives the user image previews and paste-back for file copies too,
not just for direct image copies.

## Considerations

- Multiple files: should we create one clipboard entry per image file,
  or one entry with multiple images? Probably one entry per file to
  match the "one clipboard change = one entry" model.
- Non-image files: continue to skip image capture for those (current
  behavior after the fix in d42c579).
- File size limits: large images (e.g., raw photos) could bloat
  FileStorage. May want a size cap.
