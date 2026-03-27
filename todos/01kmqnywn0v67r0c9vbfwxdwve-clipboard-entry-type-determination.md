# Rethink clipboard entry type determination

The backend currently derives `primaryFormat` from `GROUP_CONCAT(format)` via
`primary_format_from_csv` and sends it to the frontend as a string field on
`ClipboardListEntry`. The frontend uses it only for icon selection.

Questions to resolve:
- Should type determination happen in the backend or frontend?
- Should this be a derived "entry type" (text/files/image) rather than a
  "primary format"? The concepts are different — an entry can have multiple
  formats but is conceptually one type from the user's perspective.
- The current priority list (`files > image > html > rtf > text`) encodes
  policy that the frontend might want to own.
