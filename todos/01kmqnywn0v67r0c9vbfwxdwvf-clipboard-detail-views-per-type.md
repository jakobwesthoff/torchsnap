# Create distinct detail views per clipboard entry type

The detail panel currently has two modes: image (shows `<img>`) or text (shows
`<pre>` with `displayText`). File entries fall through to the text view and
show raw path strings.

Each entry type should have its own detail component:
- **Text**: current `<pre>` view (works fine)
- **Image**: current `<img>` view (works fine)
- **Files**: structured file list — filenames, paths, possibly icons per file
  type, file count header
- **HTML/RTF**: could show rendered preview or source, TBD

Also needs matching HeroIcons per type in the list view:
- text → `DocumentTextIcon` (current)
- image → `PhotoIcon` (current)
- files → `FolderIcon` or `DocumentDuplicateIcon`
- html → TBD
- rtf → TBD

Related: the `imagePath` field on `ClipboardHistoryEntry` is image-specific.
When file entries also have an "image" format (Finder icon), `imagePath` gets
set and the detail view shows the icon instead of file info. The detail view
component selection needs to be based on entry type, not on whether
`imagePath` is truthy.
